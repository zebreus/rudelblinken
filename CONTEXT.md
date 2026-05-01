# Rudelblinken

An embedded platform for synced blinking cat ears. A group of people each wear **Catears** that run user-supplied **Programs** over a WASM runtime, synchronising light patterns with each other over BLE.

## Language

### Hardware

**Catears**:
The full physical wearable assembly: 3D-printed cat ears, an LED strip, and a **Board**.
_Avoid_: device, wearable, unit, badge

**Board**:
The custom ESP32-C3 PCB.
_Avoid_: chip, microcontroller, hardware

### Software runtime

**Program**:
A user-supplied WASM binary that runs on a **Device** and defines its behaviour (light patterns, synchronisation logic, etc.).
_Avoid_: module, wasm module, binary (in domain conversations)

**Host**:
The firmware runtime that executes a **Program** and provides it access to hardware (LEDs, BLE, sensors). The counterpart to the **Guest**.
_Avoid_: firmware (when specifically discussing the runtime boundary)

**Guest**:
A **Program** as seen from the **Host** — the sandboxed WASM execution context. Use this term when discussing the host/guest boundary or the runtime contract. Use **Program** when discussing what a developer builds.
_Avoid_: module (in domain conversations)

**Fuel Metering**:
The cooperative execution model for **Programs**. A Program is allocated a fuel budget that depletes as it executes; the Program must call `yield_now()` (tentatively `refuel()`) periodically to receive more fuel from the **Host**. A Program that exhausts its fuel without yielding is terminated.
_Avoid_: watchdog (in the context of Programs), preemption, scheduling

**Device**:
Anything running the rudelblinken firmware — a physical **Board** or a software emulation.
_Avoid_: node (unless specifically discussing synchronisation), unit

**Name**:
The human-readable identifier of a **Device**. Randomly assigned from a curated list of cat-themed names on first boot; can be overridden by the user. Used by `rudelctl` and BLE clients to identify a specific Device.
_Avoid_: device name (as a compound term), label, ID

**Node**:
A **Device** acting as a participant in BLE-synchronised group behaviour.
_Avoid_: peer, device (in synchronisation contexts)

### Operations

**Upload**:
Transferring a **Program** (or other file) to a **Device**'s filesystem over BLE.
_Avoid_: flash, deploy, install (when referring to Program transfer)

**Flash**:
Writing the rudelblinken firmware to a **Board** over USB/JTAG.
_Avoid_: upload (when referring to firmware installation)

### BLE synchronisation

**Advertisement**:
A BLE broadcast packet — either received from another **Node** or sent by this **Device**. The primary mechanism by which **Programs** implement synchronisation between **Nodes**.
_Avoid_: beacon, broadcast, packet (in domain conversations)

**Synchronisation**:
The coordination of light patterns across multiple **Nodes**. Implemented entirely within **Programs** (guest-side) using **Advertisements** as the communication primitive. The **Host** provides Advertisement APIs but has no synchronisation logic of its own.
_Avoid_: sync protocol, coordination (when it could imply firmware responsibility)

## Relationships

- **Catears** contain exactly one **Board**
- A **Board** runs exactly one **Device**
- A **Device** executes exactly one **Program** at a time (falling back to the default program if none is uploaded)
- A **Device** participates as a **Node** when communicating with other **Devices** over BLE
- **Programs** are **Uploaded** to the **Device**'s filesystem; the **Firmware** is **Flashed** directly to the **Board**
- **Synchronisation** between **Nodes** is achieved entirely through **Advertisements** exchanged by **Programs**

## Example dialogue

> **Dev:** "I want to upload a new program to my catears — can I test it without the hardware first?"
> **Domain expert:** "Yes — run it in emulation using `rudelctl`. The emulated device is a full device, so the program will behave the same. Once you're happy, upload it over BLE to the real catears."

> **Dev:** "My program keeps getting killed after a few seconds — what's happening?"
> **Domain expert:** "You're probably not yielding often enough. Fuel metering means the host terminates any program that runs too long without calling `yield_now()`. Add a yield inside your main loop."

> **Dev:** "Can the host handle the synchronisation for me?"
> **Domain expert:** "No — synchronisation is entirely your program's responsibility. The host only provides the advertisement send/receive APIs. Your program decides the sync algorithm."

## Flagged ambiguities

- "device" and "node" overlap — resolved: **Device** is the runtime concept; **Node** is the networking/synchronisation concept.
