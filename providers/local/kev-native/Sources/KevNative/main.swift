// Native Kev pointer scorer. Architecture reused from pinned Apple's mlx-swift-lm.
// Author branch markers and pointer-head math are retained; this never generates text.
import Foundation
import Darwin
import MLX
import MLXNN
import MLXLLM
import MLXLMCommon

struct Branch: Decodable { let ids: [Int32]; let decide: Int; let options: [Int] }
struct Query: Decodable { let rows: [Branch] }
struct Startup: Decodable { let base: String; let adapter: String; let prepared: String; let metallib: String }
struct Reply: Encodable { let probabilities: [[Float]]; let mlxProcessPeakBytes: Int; let processPeakRssBytes: Int }
func peakRSS() -> Int { var usage = rusage(); return getrusage(RUSAGE_SELF, &usage) == 0 ? Int(usage.ru_maxrss) : 0 }
func writeJSON<T: Encodable>(_ value: T) throws {
    let data = try JSONEncoder().encode(value)
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data([10]))
}
func readFrame() throws -> Data? {
    var data = Data()
    while let bytes = try FileHandle.standardInput.read(upToCount: 1), !bytes.isEmpty {
        if bytes[0] == 10 { return data }
        if data.count >= 4 * 1024 * 1024 { throw NSError(domain: "bounded IPC", code: 1) }
        data.append(bytes)
    }
    return nil
}
func floats(_ path: URL, count: Int, shape: [Int]) throws -> MLXArray {
    let bytes = try Data(contentsOf: path)
    guard bytes.count == count * 4 else { throw NSError(domain: "head shape", code: 1) }
    let values = bytes.withUnsafeBytes { buffer in (0..<count).map { Float(bitPattern: UInt32(littleEndian: buffer.loadUnaligned(fromByteOffset: $0*4, as: UInt32.self))) } }
    guard values.allSatisfy({ $0.isFinite }) else { throw NSError(domain: "head values", code: 1) }
    return MLXArray(values, shape)
}

func run() throws {
    guard let startupData = try readFrame() else { return }
    let config = try JSONDecoder().decode(Startup.self, from: startupData)
    GPU.metallib = URL(fileURLWithPath: config.metallib)
    Memory.cacheLimit = 1 << 30
    let base = URL(fileURLWithPath: config.base)
    let object = try JSONSerialization.jsonObject(with: Data(contentsOf: base.appendingPathComponent("config.json"))) as? [String:Any]
    guard let textConfig = object?["text_config"] else { throw NSError(domain: "base config", code: 1) }
    let modelConfig = try JSONDecoder().decode(Qwen35TextConfiguration.self, from: JSONSerialization.data(withJSONObject: textConfig))
    let model = Qwen35TextModel(modelConfig)
    var arrays = try MLX.loadArrays(url: base.appendingPathComponent("model.safetensors-00001-of-00001.safetensors"))
    arrays = Dictionary(uniqueKeysWithValues: arrays.compactMap { key, value in
        if key.hasPrefix("model.visual") || key.hasPrefix("vision_tower") || key.hasPrefix("mtp.") { return nil }
        let name = key.replacingOccurrences(of: "model.language_model.", with: "model.").replacingOccurrences(of: "language_model.model.", with: "model.")
        return (name,value)
    })
    arrays = model.sanitize(weights: arrays)
    // Match Kev's merge: FP32 CPU delta, add to FP32 base, round once to BF16.
    let adapter = try MLX.loadArrays(url: URL(fileURLWithPath: config.adapter).appendingPathComponent("adapter_model.safetensors"))
    let adapterConfig = try JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: config.adapter).appendingPathComponent("adapter_config.json"))) as? [String:Any]
    guard adapterConfig?["r"] as? Int == 16, adapterConfig?["lora_alpha"] as? Int == 32, adapterConfig?["use_rslora"] as? Bool == false, adapterConfig?["trainable_token_indices"] is NSNull else { throw NSError(domain: "adapter config", code: 1) }
    for name in adapter.keys.sorted() where name.hasSuffix(".lora_A.weight") {
        let stem = String(name.dropLast(".lora_A.weight".count))
        let target = stem.replacingOccurrences(of: "base_model.model.", with: "model.") + ".weight"
        guard let old = arrays[target], let a = adapter[name], let b = adapter[stem+".lora_B.weight"] else { throw NSError(domain: "adapter target", code: 1) }
        let delta = multiply(matmul(b.asType(.float32, stream: .cpu), a.asType(.float32, stream: .cpu), stream: .cpu), Float(2.0), stream: .cpu)
        let merged = add(old.asType(.float32, stream: .cpu), delta, stream: .cpu).asType(old.dtype, stream: .cpu)
        eval(merged)
        arrays[target] = merged
    }
    try model.update(parameters: ModuleParameters.unflattened(arrays), verify: [.all])
    arrays.removeAll()
    eval(model.parameters())
    Memory.clearCache()
    let prepared = URL(fileURLWithPath: config.prepared)
    let qw = try floats(prepared.appendingPathComponent("q.weight.f32"), count: 256*1024, shape: [256,1024])
    let qb = try floats(prepared.appendingPathComponent("q.bias.f32"), count: 256, shape: [256])
    let kw = try floats(prepared.appendingPathComponent("k.weight.f32"), count: 256*1024, shape: [256,1024])
    let kb = try floats(prepared.appendingPathComponent("k.bias.f32"), count: 256, shape: [256])
    try writeJSON(["ok":true])
    while let input = try readFrame() {
        let query = try JSONDecoder().decode(Query.self, from: input)
        guard (1...8).contains(query.rows.count), query.rows.allSatisfy({ row in row.ids.count <= 16384 && row.decide >= 0 && row.decide < row.ids.count && (2...8).contains(row.options.count) && row.options.allSatisfy({ index in index>=0 && index<row.ids.count })}) else { throw NSError(domain:"row bounds",code:1) }
        var probabilities = [[Float]]()
        let maximum = query.rows.map { $0.ids.count }.max()!
        let rowsPerPass = max(1, 16384 / maximum)
        for start in stride(from: 0, to: query.rows.count, by: rowsPerPass) {
            let rows = Array(query.rows[start..<min(start+rowsPerPass,query.rows.count)])
            let length = rows.map { $0.ids.count }.max()!
            // Right padding cannot affect real tokens in either causal layer type.
            let tokenValues = rows.flatMap { $0.ids + Array(repeating: Int32(0), count: length-$0.ids.count) }
            let tokens = MLXArray(tokenValues, [rows.count,length])
            var state = LMOutput.State()
            state[mtpEmitFlagKey] = true
            // Independent full causal rows, avoiding any home-grown hybrid cache implementation.
            let output = model(LMInput.Text(tokens: tokens), cache: nil, state: state)
            guard let hidden = output.state?[mtpLastHiddenStatesKey], hidden.shape == [rows.count,length,1024] else { throw NSError(domain:"hidden state",code:1) }
            for (index,row) in rows.enumerated() {
            let decide = hidden[index,row.decide,0...].asType(.float32,stream:.cpu)
            let q = add(matmul(qw,decide,stream:.cpu),qb,stream:.cpu)
            let keys = MLX.stacked(row.options.map { hidden[index,$0,0...].asType(.float32,stream:.cpu) })
            let k = add(matmul(keys,kw.transposed(),stream:.cpu),kb,stream:.cpu)
            let logits = multiply(matmul(k,q,stream:.cpu), Float(1.0/16.0/2.3510958125672174), stream:.cpu)
            let p = softmax(logits,stream:.cpu)
            eval(p)
            let values=p.asArray(Float.self)
            guard values.allSatisfy({$0.isFinite && $0>=0 && $0<=1}) else {throw NSError(domain:"probabilities",code:1)}
            probabilities.append(values)
            }
        }
        try writeJSON(Reply(probabilities: probabilities, mlxProcessPeakBytes: Memory.peakMemory, processPeakRssBytes: peakRSS()))
    }
}
do { try run() } catch { exit(1) } // Native errors may contain source text; suppress every diagnostic.
