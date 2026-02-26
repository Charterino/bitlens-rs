# bitlens-rs

This is the backend for bitlens, a small pet-project I've been working on over the last few months.
It's a very simple bitcoin explorer that connects to other peers, syncs headers, blocks, and transactions, as well as gossips about other peers.

Check it out at https://bitlens.charterino.ru

Currently implemented:
- bitcoin packet protocol (see src/packet)
- header sync (see src/chainman/headersync.rs)
- block sync (see src/chainman/blocksync.rs)
- rest api calls to retrieve the stored data (see src/api)
- some prometheus metrics (see src/metrics/mod.rs)
- github actions runs to build and package and publish the docker image
- (rudimentary) block and transaction verification

Environment variables:
| ENV | Description | Default |
| --- | --- | --- |
| INITIAL_LARGE_DESERIALIZE_ARENAS_COUNT | The number of large (4mb) memory chunks used to deserialize incoming packets | 64 (256mb) |
| LARGE_DESERIALIZE_ARENAS_COUNT_BLOCKSYNC | The number of large (4mb) memory chunks during blocksync | 512 (2gb) |
| INITIAL_SMALL_DESERIALIZE_ARENAS_COUNT | The number of small (8kb) memory chunks used to deserialize incoming packets | 256 (2mb) |
| SMALL_DESERIALIZE_ARENAS_COUNT_BLOCKSYNC | The number of small (8kb) memory chunks during blocksync | 1024 (8mb) |
| MAX_DOWNLOAD_WORKERS | The number of tasks used to download blocks during blocksync | 100 |
| MAX_BLOCKS_PER_FLUSH | The number of blocks per write during blocksync | 100 |
| ROCKSDB_BUFFER_SIZE | The size of rocksdb cache, in bytes | 9000000000 (9gb) |
| METRICS_LISTEN_ADDRESS | The address on which to run the metrics server for prometheus | 127.0.0.1:1281 |
| METRICS_TOKEN | Token to access the metrics data | |
| DISABLE_INSUFFICIENT_ARENAS_LOG | Disables logs about insufficient arenas | false |