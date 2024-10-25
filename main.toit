import gpio
import gpio.pwm
import math
import esp32
import log
import core.utils
import net
import esp32.espnow
import device
import encoding.json
import encoding.ubjson
import system.firmware
import system.api.containers
import system.assets
import system
import encoding.tison
import .sync
import .firefly
import .communication
import .led
import .ambient-light

INTERVAL ::= Duration --us=100

TARGET-SPEED ::= 1000
// Length of a subcycle in us
speed := TARGET-SPEED
CYCLE-LENGTH ::= 5000
// How many cycle steps were taken
cycle-progress := 0
// How many us were already spend in this substep
unprocessed-delta := 0


device-name:
  config := {:}
  assets.decode.get "config" --if-present=: | encoded |
      catch: config = tison.decode encoded
  return config.get "name"

main args:
  init-communication
  print "Hey, my name is $device-name"

  firefly := Firefly device-name 1000000
  firefly.dampening = 0.95
  task::
    ambient-light-task
  task::
    receiver-task::
      firefly.receive-pulse it
  last-time := Time.monotonic-us --since-wakeup=true
  while true:
    catch --trace:
      time := Time.monotonic-us --since-wakeup=true
      delta := time - last-time
      last-time = time

      firefly.tick delta
      brightness := firefly.brightness
      // print brightness
      set-brightness (1 + ( math.sin ((brightness * 6.14159) / 255.0)))/2.0

      sleep INTERVAL