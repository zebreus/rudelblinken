import gpio
import pixel-strip show PixelStrip
import .ambient-light
import math

// How many ws2812 LEDs are connected
PIXELS ::= 70
// To which pin the LEDs are connected
PIN ::= gpio.Pin 7
// Size until the rainbow repeats. Lower is bigger
RAINBOW-SIZE ::= 0.2
// How fast the rainbow moves. Lower is faster
RAINBOW-SPEED ::= 60
// Limit the maximum brightness. 1 is full brightness
BRIGHTNESS-FACTOR := 0.5
// Limit the minimum brightness. 0 is off, 255 is always full brightness
MIN-BRIGHTNESS := 2

strip := PixelStrip.uart PIXELS --pin=PIN
r := ByteArray PIXELS
g := ByteArray PIXELS
b := ByteArray PIXELS
current := 0
brightness/int := 10
set-brightness new-brightness/float:
  brightness-factor := BRIGHTNESS-FACTOR

  adjusted-brightness := new-brightness.abs * brightness-factor
  pwm-brightness := math.pow adjusted-brightness 1.0
  brightness = (pwm-brightness * (255 - MIN-BRIGHTNESS)).to-int + MIN-BRIGHTNESS
  print brightness


  progress := current.to-float / RAINBOW-SPEED.to-float
  
  PIXELS.repeat:
    part-of-that-calculation := (math.PI * progress * 2) + RAINBOW-SIZE * it
    r[it] = ((math.pow (((math.sin part-of-that-calculation + (math.PI * 0.33 * 2)) + 1) / 2) 2) * brightness).to-int;
    g[it] = ((math.pow (((math.sin part-of-that-calculation + (math.PI * 0.33 * 4)) + 1) / 2) 2) * brightness).to-int;
    b[it] = ((math.pow (((math.sin part-of-that-calculation) + 1) / 2) 2) * brightness).to-int;
  // Show the current configuration.
  strip.output r g b
  // Prepare for the next round.
  current = (current + 1) % (RAINBOW-SPEED)