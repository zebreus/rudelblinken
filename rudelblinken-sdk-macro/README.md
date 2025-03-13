<!-- cargo-rdme start -->

Generate the boilerplate for a rudelblinken program written in Rust.

# Example

```rust
use rudelblinken_sdk_macro::{main, on_advertisement};

#[main]
pub fn main() {
    println!("Hello, world!");
}

#[on_advertisement]
fn on_advertisement(_: rudelblinken_sdk::Advertisement) {
    // Do something with the advertisement
    println!("Got an advertisement!");
}
```

This expands to something roughly like this:

```rust
// Setup a custom allocator, because we can only use one page of
// memory but that is not supported by the default allocator
const HEAP_SIZE: usize = 36624;
static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];
#[global_allocator]
static ALLOCATOR: ::talc::Talck<::spin::Mutex<()>, ::talc::ClaimOnOom> =
    ::talc::Talc::new(unsafe {
        ::talc::ClaimOnOom::new(::talc::Span::from_array((&raw const HEAP).cast_mut()))
    })
    .lock();

// We need a main function to be able to `cargo run` this project
#[allow(dead_code)]
fn main() {}

// Define the struct that will implement the `Guest` and `BleGuest` traits
struct RudelblinkenMain;

// Generate WASM exports for the `RudelblinkenMain` struct
mod _generated_exports {
    use super::RudelblinkenMain;
    use ::rudelblinken_sdk::{export, exports};
    ::rudelblinken_sdk::export! {RudelblinkenMain}
}

// Implement the `Guest` trait for the `RudelblinkenMain` struct
impl ::rudelblinken_sdk::Guest for RudelblinkenMain {
    fn run() {
        println!("Hello, world!");
    }
}

// Implement the `BleGuest` trait for the `RudelblinkenMain` struct
impl rudelblinken_sdk::BleGuest for RudelblinkenMain {
    fn on_advertisement(_: rudelblinken_sdk::Advertisement) {
        // Do something with the advertisement
        println!("Got an advertisement!");
    }
}
```

### Other languages

If you want more control over the generated code, you can also use the

<!-- cargo-rdme end -->
