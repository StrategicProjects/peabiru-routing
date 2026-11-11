# Bundlers and loading the module

The package is two parts: the JavaScript API (`lib/index.js`, with the
worker script `lib/worker.js`) and the WebAssembly module
(`pkg/peabiru_bg.wasm`, about 670 KB, 280 KB compressed). Both are found
from `import.meta.url`, so in most setups there is nothing to configure.

## Where the module comes from

`buildNetwork()` loads the module the first time it is called:

- in Node, from the file next to the package;
- elsewhere, with `fetch()` from
  `new URL("../pkg/peabiru_bg.wasm", import.meta.url)`, a form that
  bundlers recognise and copy to their output.

When the file is served from somewhere else (a CDN, another path), call
`init()` before anything else, with a URL, a `Response`, the bytes or a
compiled `WebAssembly.Module`:

```js
{{#include ../examples/browser-init.mjs:init}}
```

The package also exports the file as `peabiru/peabiru_bg.wasm`, for
bundlers that import assets by name. `initSync(bytes)` loads it
synchronously, but some browsers do not compile large modules
synchronously on the main thread: prefer `init()` there. The workers
receive the module already compiled, so they do not fetch the file again.

## Vite

Import the package as in Node; `vite build` emits the `.wasm` file and the
worker script with the bundle, and nothing needs configuring:

```js
import * as peabiru from "peabiru";
```

The browser examples of this guide are checked this way: `run_examples.mjs`
bundles them, with the test page, with `vite build` (when `VITE` names a
Vite executable) and runs the bundle in headless Chrome, with a pool of
workers. That has been done with Vite 8.3 and Chrome; the bundle gives the
results of the calling thread.

## Other bundlers and browsers

The worker is loaded with
`new Worker(new URL("./worker.js", import.meta.url), { type: "module" })`,
which webpack 5 also recognises, as it recognises the `new URL(...)` of the
`.wasm` file. **webpack has not been tested**, and neither have other
bundlers. **Firefox and Safari have not been tested either**: the package
needs module workers (`{ type: "module" }`), WebAssembly and ES modules,
which current versions of both support, but the examples run only in
Chrome.

In plain ES modules (no bundler) the worker script and the `.wasm` file are
fetched next to `lib/index.js`; map the package name with an import map, as
in [Getting started](getting-started.md#in-a-browser), and serve the
package's `lib/` and `pkg/` directories together.

## Node

In Node (18 or later) the package reads the `.wasm` file from disk and
starts its workers with `worker_threads`; there is nothing to configure.
It is an ES module only: `import` it (from CommonJS,
`await import("peabiru")`); `require()` does not work before Node 22.

## Cross-origin isolation

A pool of workers keeps the input bytes in a `SharedArrayBuffer` when one
is available, so that every worker reads one copy. Pages have
`SharedArrayBuffer` only when they are cross-origin isolated (served with
the `Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp` headers); otherwise the input
is copied to each worker instead, which works the same and costs more
memory (see [Threads and workers](threads.md#memory)).
