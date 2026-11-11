# Threads and workers

WebAssembly runs on one thread. To route on several, peabiru starts a pool
of workers (module Web Workers in browsers, `worker_threads` in Node), each
with its own copy of the network, and shares the origins of every call out
among them.

## The calling thread (the default)

By default (`nThreads: 1`) a network has no workers and routes on the
calling thread: the memory is predictable, but a large matrix blocks the
thread (a page freezes) until it is done, and it cannot be stopped. Keep
it for small networks, scripts and tight memory.

## A pool of workers

With `nThreads` above 1, `buildNetwork()` starts that many workers and
builds the network in each, all at once:

```js
{{#include ../examples/threads.mjs:pool}}
```

- `nThreads: 0`, `null` or `Infinity` start one worker per core
  (`navigator.hardwareConcurrency`, or `os.availableParallelism()` in
  Node).
- The `nThreads` of a routing call is the number of the network's workers
  it uses: all of them by default (null or `Infinity`), all it has if more
  are asked for. A network without workers ignores it.
- A call shares its origins out in contiguous blocks, one per worker, and
  puts the columns back together in origin order. Calls made while the
  workers are busy wait for them; several calls can be in flight on one
  network.
- `network.free()` terminates the workers (and rejects the calls still
  running). In Node an idle worker does not keep the program alive, so a
  script can also end without calling it.

The results are those of the calling thread exactly (same rows, same
order, same values): the core routes every origin on its own, and the
Monte Carlo draws of an origin are seeded from the `seed` and the origin's
coordinates, not from its position in the input.

```js
{{#include ../examples/threads.mjs:same}}
```

```text
{{#include ../examples/output/threads.txt:1:2}}
```

Street matrices: each worker decides on its own share of the origins
whether to build a contraction hierarchy, which it then keeps for later
calls. With many workers and a few hundred origins each, the first one or
two street matrices of a mode may go by plain search before the hierarchy
is built. The results are the same either way.

## Stopping a call

Every call takes a `signal`, an `AbortSignal`. On abort the call rejects
with the signal's reason (an `AbortError` for `controller.abort()`), its
workers are terminated at once, and new ones are built in the background
from the same input; the network stays usable, and later calls wait for
the new workers:

```js
{{#include ../examples/threads.mjs:abort}}
```

```text
{{#include ../examples/output/threads.txt:3:4}}
```

A running WebAssembly computation cannot be interrupted any other way, so
on the calling thread the signal is checked only before the call starts.
To be able to stop calls on one thread, start a single worker with
`useWorkers: true`:

```js
{{#include ../examples/threads.mjs:one-worker}}
```

```text
{{#include ../examples/output/threads.txt:5}}
```

Errors raised in a worker reach the caller with the class and message they
have on the calling thread.

## Memory

Each worker holds a whole network, about as much as a network on the
calling thread, plus its own WebAssembly instance. A pool also keeps one
copy of the input bytes (`.pbf` and GTFS), to rebuild a worker after an
abort: in a `SharedArrayBuffer` that every worker reads where one is
available (Node, and pages that are
[cross-origin isolated](https://developer.mozilla.org/en-US/docs/Web/API/Window/crossOriginIsolated)),
otherwise as a copy sent to each worker, which drops it once its network is
built.

Measured on the r5r sample of Porto Alegre (`.pbf` 0.9 MB, two GTFS feeds
1.1 MB, 1227 points), in Node 24 on an Apple M2 Pro, peak resident memory of
the whole process (build and one transit matrix), as reported in the
package's README:

| Workers | Peak memory | Input sent as |
|---|---|---|
| none (calling thread) | 287 MiB | - |
| 1 | 336 MiB | `SharedArrayBuffer` |
| 2 | 408 MiB | `SharedArrayBuffer` |
| 4 | 563 MiB | `SharedArrayBuffer` |
| 1 | 412 MiB | copies |
| 4 | 627 MiB | copies |

That is about 75 MiB more per worker on this network; a larger network
costs more per worker, roughly in proportion to its size. Use workers for
city-sized matrices and for pages that must stay responsive; keep the
default for small networks and tight memory.

## Workers of your own

A `Network` cannot be sent to another Worker (`postMessage` does not copy
it). To use peabiru inside a Worker of your own, import it and build the
network there.
