# rudelblinken-filesystem

<!-- cargo-rdme start -->

A zero-copy flash filesystem optimized for embedded systems

`rudelblinken-filesystem` implements a flash-friendly filesystem designed for resource-constrained
embedded devices. Key features include:

- **Zero-copy access**: Files are memory-mapped for direct, efficient access
- **Flash-optimized**: Implements wear leveling and flash-aware write patterns
- **Safe concurrency**: Reference counting enables safe concurrent access with reader/writer separation. Deferred deletion ensure data integrity
- **Resource efficient**: Minimal RAM overhead during normal operation

The filesystem provides direct memory-mapped access to file contents while maintaining safety
through a custom reference counting system. Multiple readers can access files concurrently
while writers get exclusive access. Files are only deleted once all references are dropped.

Designed specifically for flash storage, the implementation uses block-aligned operations,
respects write limitations, and implements basic wear leveling.

<!-- cargo-rdme end -->
