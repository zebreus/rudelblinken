import gpio
import gpio.pwm

pin := gpio.Pin 8
generator := pwm.Pwm --frequency=5000
channel := generator.start pin

set-brightness brightness/float:
  channel.set-duty-factor brightness