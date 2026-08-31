# asyncritty

`asyncritty` is a Rust library that integrates
[`alacritty_terminal`](https://crates.io/crates/alacritty_terminal) with Tokio.
It runs processes on pseudoterminals (PTYs), handles asynchronous input and
output, and parses terminal output into Alacritty's terminal model for
applications to inspect or render.

The project aims for broad UNIX compatibility, but has only been tested on Linux
so far. Windows support is planned but not yet implemented.

Licensed under the [Apache License 2.0](https://opensource.org/license/Apache-2.0).
