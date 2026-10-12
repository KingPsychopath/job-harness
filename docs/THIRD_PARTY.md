# Dependencies and optional artifacts

Job Harness's reusable source is MIT-licensed. Dependencies, model weights, fonts and native vendor runtimes retain their own licenses.

Native release staging records each component's resolved normal/build Cargo dependencies, versions, declared license expressions and available license text. Archives include a third-party inventory, notices and collected license files. Coverage is explicit; the inventory is not a claim to resolve every legal obligation. Build-only and platform-specific packages may appear in a component's resolved dependency set.

Vendor model runtimes and weights are not included in the core, desktop or provider software archives. Their explicit install plans identify the publisher, pinned source, digest, size and license evidence. Review that plan before installation. A library extracted from a publisher's wheel is still the publisher's native library; this does not require or install its Python source/interpreter.

The optional Kev worker uses separately pinned Swift/MLX dependencies and a model-specific weight plan. Its source/build guide lists that distinct boundary. Metadata-only alternatives are not supported runtime claims.

The CV generator accepts selected fonts and retains source evidence. Its synthetic example is not a candidate CV. A private workspace bundle can contain its owner's supplied artifacts; permission to transfer or share those is separate from the toolkit license.

See [model details](MODELS.md), [CVs](CV.md), [native installation](INSTALL.md) and [private transfers](BUNDLES.md).
