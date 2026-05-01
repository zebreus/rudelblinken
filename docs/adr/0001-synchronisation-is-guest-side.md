# Synchronisation is implemented entirely in Programs, not in the Host

The Host firmware provides BLE Advertisement APIs (send and receive) but contains no synchronisation logic. All coordination of light patterns across Nodes — timing, phase alignment, leader election, etc. — is the responsibility of Programs running on the Guest side. This is a deliberate extension of the WASM sandbox principle: the Host provides primitives, Programs provide behaviour.

## Considered options

Host-managed sync: the firmware could maintain a shared clock or sync state and expose a higher-level "set group phase" API. Rejected because it would couple the firmware to a specific synchronisation algorithm, preventing Programs from experimenting with different approaches. The guest-side model keeps the firmware stable while allowing arbitrary sync strategies to be deployed as Programs.
