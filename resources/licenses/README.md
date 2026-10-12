# Pinned dependency notices

Some published crates omit the repository's license files. This cache contains unchanged upstream notices tied to each crate's exact `.cargo_vcs_info.json` commit and explicitly referenced versioned standard license bodies. It contains software licensing evidence, never workspace or candidate data.

[`index.json`](index.json) binds each text to the published package name, version, declared license, repository, crate path, commit, byte count and SHA256. Release staging remains offline: it verifies that binding against fetched Cargo metadata and the published crate's VCS evidence, then checks each cached text's digest. A matching entry with a different license, repository or commit fails. An uncached version remains an explicit coverage gap instead of reusing an unrelated notice.

| Package | Exact upstream source |
| --- | --- |
| agent-client-protocol-schema 1.11.0 | [LICENSE](https://github.com/agentclientprotocol/agent-client-protocol/blob/4cf3dd858c819fc3ab99ae53f76883ede0345a10/LICENSE) |
| defmt-parser 1.0.0 | [MIT](https://github.com/knurling-rs/defmt/blob/4a8cdb44891ed57b8ff5a023b6bec7137c48708f/LICENSE-MIT), [Apache](https://github.com/knurling-rs/defmt/blob/4a8cdb44891ed57b8ff5a023b6bec7137c48708f/LICENSE-APACHE) |
| jsonschema-regex 0.46.10 | [LICENSE](https://github.com/Stranger6667/jsonschema/blob/ecaeceac2340908a8fbf71404442296bd1536520/LICENSE) |
| rmcp 3.5.1 | [LICENSE](https://github.com/modelcontextprotocol/rust-sdk/blob/79437f291b2c44053d00dcd5db969fd0cca7c887/LICENSE) |
| uuid-simd and vsimd 0.8.0 | [LICENSE](https://github.com/Nugine/simd/blob/d74c030d9dc4f3cae02146d1f497ff62726ef09a/LICENSE) |
| objc2 family | Each published crate's exact `LICENSE.md` commit is recorded in the index. These unchanged publisher statements link their MIT/Zlib/Apache license choices; they do not supply a complete copyright inventory. |
| selectors 0.38.0 | [Published source header, lines 1–3](https://github.com/servo/stylo/blob/572ecba2d1600e7c3d490586692a209faf703baa/selectors/lib.rs#L1-L3), plus its referenced [Mozilla MPL-2.0 text](https://www.mozilla.org/media/MPL/2.0/index.815ca599c9df.txt) |

The selectors declaration is checked against the complete published source file's SHA256 and exact line bytes. Its standard license body has separate provenance: a versioned publisher URL, pinned digest and a verified reference in that declaration. It is not described as a file from the Stylo repository. Publisher licensing summaries, source declarations and standard license bodies retain distinct content roles in the inventory.

`unicode-casefold` 0.2.0 declares `MIT/Apache-2.0`, but its published crate has no VCS metadata or license file. Its public repository, now [lambda-fairy/unicode-casefold](https://github.com/lambda-fairy/unicode-casefold), also contains no license text. That gap remains explicit in archive coverage; no copyright notice or license text is invented.

These texts retain their upstream licenses. Collection is not a legal-compliance assessment and excludes separately downloaded model weights and vendor runtimes.
