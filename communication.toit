import esp32.espnow
import encoding.json
import encoding.ubjson
import system.storage
import .upgrade

PMK ::= espnow.Key.from-string "mustbe16bytesaaa"

service/espnow.Service := ?

class Pulse:
  // Unique name of the sender
  sender /string := ""
  // The sender usually sends the pulse a few millis after it actually pulsed
  // This field specifies how old the pulse was when it was send
  ago /int := 0
  counter /int := 0
  // Communicate the own preferred pulse length
  length /int := 0
  // If this bit is set, override the preferred pulse length is changed
  dom /int := 0
  dom-age /int := -1

  stringify:
    return json.stringify {
      "sender": sender,
      "ago": ago,
      "length": length,
      "counter": counter,
      "dom": dom,
      "dom-age": dom-age
    }
  
  encode:
    return json.encode {
      "sender": this.sender,
      "ago": this.ago,
      "counter": this.counter,
      "length": this.length,
      "dom": this.dom,
      "dom-age": this.dom-age,
      "pulsev1": 1
    }
  
  decode received-data/Map:
    if received-data["pulsev1"] != 1:
      return false
    this.ago = received-data["ago"].to-int
    this.counter = received-data["counter"].to-int
    this.sender = received-data["sender"]
    if received-data.contains "dom":
      this.dom = received-data.get "dom" --if-absent=: 0
      this.dom-age = received-data.get "dom-age" --if-absent=: -1
    if received-data.contains "upgrade":
      upgrade := received-data.get "upgrade" --if-absent=: 0
      if upgrade == 9428:
        require-upgrade

    this.length = received-data.get "length" --if-absent=: 0
    return true

send-pulse pulse/Pulse:
  service.send
    pulse.encode
    --address=espnow.BROADCAST-ADDRESS

receiver-task on-pulse/Lambda:
  count := 0
  while true:
    catch --trace:
      datagram := service.receive
      received-data := json.decode datagram.data
      pulse := Pulse
      decoded-successfully := pulse.decode received-data

      if decoded-successfully:
        on-pulse.call pulse
    
      print "Receive datagram from \"$datagram.address\", data: \"$pulse\""

init-communication:
  service = espnow.Service.station --key=PMK --channel=6
  service.add-peer espnow.BROADCAST-ADDRESS