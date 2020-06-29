# rain-llvm
[![Pipeline status](https://gitlab.com/rain-lang/rain-llvm/badges/master/pipeline.svg)](https://gitlab.com/rain-lang/rain-llvm)
[![codecov](https://codecov.io/gl/rain-lang/rain-llvm/branch/master/graph/badge.svg)](https://codecov.io/gl/rain-lang/rain-llvm)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

`rain-llvm` is a crate implementing `rain` to LLVM codegeneration, i.e. the conversion of `rain` IR stored in memory to `llvm` IR. 
We use [the experimental `inkwell` library](https://github.com/TheDan64/inkwell) to generate LLVM: 
please reference the documentation (WIP) for a more in-depth overview of our code generation strategy.
You can find more information on `rain` in [the repository](https://gitlab.com/tekne/rain).

Contributions, ideas and collaboration proposals are welcome: please make an issue or e-mail jad.ghalayini@mail.utoronto.ca.
