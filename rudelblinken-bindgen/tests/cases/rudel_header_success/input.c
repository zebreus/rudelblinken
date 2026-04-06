#pragma once

_Static_assert(sizeof(int) == 4, "int needs to be i32");
_Static_assert(sizeof(long long) == 8, "long long needs to be i64");

enum rudel_log_level { ERROR = 0, WARNING = 1, INFO = 2, DEBUG = 3, TRACE = 4 };

enum hardware_version {
  UNKNOWN = 0,
};

/// UTF-8 encoded string
struct rudel_string {
  char *ptr;
  unsigned int len;
};

struct rudel_advertisement {
  unsigned char address[6];
  unsigned char company[2];
  unsigned char data[50];
  unsigned int length;
  unsigned long long received_at;
};

/// TODO: figure out a better interface for this function
unsigned int rudel_get_base_version();
/// Yield now
///
/// Returns the remaining fuel after yielding
unsigned int rudel_yield_now(unsigned long long micros);
/// Returns the remaining fuel
unsigned int rudel_get_remaining_fuel();
/// Sleeps without yielding
///
/// Returns the remaining fuel after sleeping
unsigned int rudel_sleep(unsigned long long micros);
/// Get the elapsed micros since boot
unsigned long long rudel_time();
/// Log a message
///
/// Message is a null-terminated UTF-8 string
void rudel_log(enum rudel_log_level log_level, char *message);
/// Get the device name
///
/// Device name is a 16 byte pointer
/// The name will be placed there as an UTF-8 encoded string
/// Returns the length of the name in bytes
unsigned int rudel_get_name(char device_name[16]);
/// Get the system configuration data
/// Fills the buffer with configuration data
///
/// Returns the number of bytes written to the buffer
unsigned int rudel_get_config(unsigned char data[200]);
/// Get the rudelblinken hardware version
enum hardware_version rudel_get_hardware_version();
/// Set LED intensities starting from first-id
void rudel_set_led(unsigned int led_id, unsigned int brightness);
// rudel_errno_t rudel_set_rgb(rudel_led_color_t *color, uint32_t lux);
/// Get the number of LEDs
unsigned int rudel_led_count();
/// Get the current ambient light in lux
unsigned int rudel_get_ambient_light();
/// Get the current reading of the vibration sensor
unsigned int rudel_get_vibration();
/// Get the current reading of the voltage sensor in millivolts
unsigned int rudel_get_voltage();
/// Configure the BLE advertisement settings
void rudel_configure_advertisement(unsigned int min_interval,
                                   unsigned int max_interval);
/// Set the BLE advertisement data (up to 32 bytes)
void rudel_set_advertisement_data(unsigned char *data, unsigned int length);
/// Get the latest received and unprocessed advertisement
///
/// Each received advertisement is only delivered at most once.
///
/// The exact semantics of this function is not yet defined. Try to call it as
/// often as possible to not miss any advertisements.
///
/// Returns the number of advertisements that were available to read. If zero,
/// the value of `advertisement` is was not modified. If non-zero,
/// `advertisement` was filled with a valid advertisemend.
unsigned int rudel_get_advertisement(struct rudel_advertisement *advertisement);

/// The main function
void rudel_run(void);
