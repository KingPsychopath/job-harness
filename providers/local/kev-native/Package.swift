// swift-tools-version: 6.2
import PackageDescription
let package = Package(
 name: "KevNative", platforms: [.macOS(.v14)],
 products: [.executable(name: "kev-native", targets: ["KevNative"])],
 dependencies: [
  .package(url: "https://github.com/ml-explore/mlx-swift-lm", revision: "3b339ad6e3b3f44c8121ecff5131c7fd55e075e6"),
  .package(url: "https://github.com/ml-explore/mlx-swift", revision: "19601207e9a0de51e03ee6ec0c3c5f3784275075")
 ],
 targets: [.executableTarget(name: "KevNative", dependencies: [
  .product(name: "MLXLLM", package: "mlx-swift-lm"),
  .product(name: "MLXLMCommon", package: "mlx-swift-lm"),
  .product(name: "MLX", package: "mlx-swift"),
  .product(name: "MLXNN", package: "mlx-swift")
 ])]
)
