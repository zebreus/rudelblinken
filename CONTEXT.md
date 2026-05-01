# Rudelblinken

An embedded platform for synced blinking cat ears. A group of people each wear **Catears** that run user-supplied **Programs** over a WASM runtime, synchronising light patterns with each other over BLE.

## Language

**Rudelblinken**:
The name of the project and platform. "Rudel" is German for pack/group; "blinken" means to blink/flash. The platform enables a group of people wearing **Catears** to synchronise light patterns with each other.

**Rudel**:
The group of people wearing **Catears** together. All **Nodes** in a Rudel synchronise their light patterns with each other via **Advertisements**.
_Avoid_: group, pack, network (in domain conversations)

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

**SDK**:
The Rust library (`rudelblinken-sdk`) for writing **Programs**. Provides bindings to the **Host** APIs. Not yet stable.
_Avoid_: library (use SDK)

**Main Program**:
The **Program** a **Device** is configured to run, identified by its hash stored in persistent config. Set via BLE (e.g. using `rudelctl`). May be unset.
_Avoid_: active program (when discussing configuration), current program

**Default Program**:
A WASM binary baked into the firmware flash partition (not the **Device** filesystem) that provides fallback blinking behaviour. Runs when no **Main Program** is set or when the **Main Program** repeatedly fails to start.
_Avoid_: default file, fallback program (in domain conversations)

**Active Program**:
The **Program** a **Device** is currently executing — either the **Main Program** or the **Default Program**.
_Avoid_: current file, selected file

**Program Failure**:
A **Main Program** run that exits before having run continuously for the success duration (~30 s). Any exit counts — crash, panic, fuel exhaustion, or clean return. The **Device** tracks consecutive failures; after too many, the **Main Program** is automatically deleted and the **Device** falls back to the **Default Program**. Because each power-cycle leaves the failure flag set, repeatedly unplugging and replugging the **Device** accumulates failures and provides a hardware escape hatch to reset to the **Default Program** without `rudelctl`.
_Avoid_: crash (implies unclean exit only — a clean-but-premature exit is also a failure)

**Host**:
The firmware runtime that executes a **Program** and provides it access to hardware (LEDs, BLE, sensors). The counterpart to the **Guest**.
_Avoid_: firmware (when specifically discussing the runtime boundary)

**Firmware**:
The rudelblinken software that runs on a **Board** — includes the **Host** runtime, BLE stack, filesystem, and the **Default Program**. Distinct from user-supplied **Programs**.
_Avoid_: software (too generic), program (conflicts with user Programs)

**Guest**:
A **Program** as seen from the **Host** — the sandboxed WASM execution context. Use this term when discussing the host/guest boundary or the runtime contract. Use **Program** when discussing what a developer builds.
_Avoid_: module (in domain conversations)

**Fuel Metering**:
The cooperative execution model for **Programs**. A Program is allocated a fuel budget that depletes as it executes; the Program must call `yield_now()` (tentatively `refuel()`) periodically to receive more fuel from the **Host**. A Program that exhausts its fuel without yielding is terminated.
_Avoid_: watchdog (in the context of Programs), preemption, scheduling

**Device**:
Anything running the rudelblinken firmware — a physical **Board** or a software emulation.
_Avoid_: node (unless specifically discussing synchronisation), unit

**Device Name**:
The human-readable identifier of a **Device**. Randomly assigned from a curated list of cat-themed names on first boot; can be overridden by the user. Used by `rudelctl` and BLE clients to identify a specific Device.
_Avoid_: Name (without Device, because it conflicts with **Filename**), label, ID

**Node**:
A **Device** acting as a participant in BLE-synchronised group behaviour.
_Avoid_: peer, device (in synchronisation contexts)

### Device filesystem

**File**:
Stored content in a **Device** filesystem. A **Program** is a **File** whose contents can be executed by the **Host**; other Files may support Upload or runtime behaviour.
_Avoid_: file when referring to source files in this repository; binary (when you mean Program)

**Filename**:
The storage identifier of a **File** in the **Device** filesystem.
_Avoid_: **Device Name**, path

**Important File**:
A **File** the **Device** should not automatically delete to make room for another Upload. Files are unimportant by default unless explicitly marked important. Important Files are used sparingly because Files cannot currently be made unimportant again; Main Programs and temporary checksum Files are not automatically marked important.
_Avoid_: retained file, protected file, pinned file

**File Age**:
A per-**File** counter that tracks how long a File has been on the **Device** filesystem. When the **Device** needs space for a new **Upload**, it evicts the oldest unimportant **Files** first. Important Files are never evicted regardless of age.
_Avoid_: timestamp (age is a relative counter, not a wall-clock time)

### Operations

**rudelctl**:
The command-line tool for interacting with **Devices** over BLE — uploading **Programs**, setting the **Main Program**, and inspecting **Device** state. Only runs on Linux. Also referred to as "the CLI".
_Avoid_: control tool

**Service**:
A BLE GATT service exposed by a **Device**. The **File Upload Service** handles **Uploads**; the **Cat Management Service** manages the **Main Program**, **Device Name**, and other **Device** configuration.
_Avoid_: endpoint, API (in domain conversations)

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
- A **Rudel** is a group of **Nodes** synchronising together
- A **Device** may store multiple **Programs**, but executes only one **Active Program** at a time — the **Main Program** if one is configured, otherwise the **Default Program**
- A **Program** is a **File** that can be executed by the **Host**
- A **Device** participates as a **Node** when communicating with other **Devices** over BLE
- **Programs** are **Uploaded** to the **Device**'s filesystem; the **Firmware** is **Flashed** directly to the **Board**
- **Important Files** are not automatically deleted when the **Device** needs filesystem space for another Upload
- When making room for an **Upload**, the **Device** evicts the oldest unimportant **Files** first (by **File Age**)
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
- "file" is ambiguous between repository source files and stored content on a Device — resolved: **File** is a Device filesystem concept; use "source file" or a path when discussing files in this repository.
- "name" is ambiguous between a **Device Name** and a **Filename** — resolved: use the full term.
