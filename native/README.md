# Native adapters

This directory is reserved for explicit Linux and Windows native adapter
interfaces. It currently contains no native collection or operating-system access.

Under the [Linux-first roadmap](../docs/linux-first-roadmap.md), revised Version 5
will implement `native/linux/` using the shared collector/policy interfaces.
Distribution-specific service/log facilities stay behind small typed adapters.
Windows collection is deferred into Version 16. No adapter is implemented or
enabled by this documentation change.
