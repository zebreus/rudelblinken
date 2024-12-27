import system.storage

STORAGE ::= storage.Bucket.open --flash "rudelblinken"

enforce-upgrade:
  upgrade-counter := STORAGE.get "upgrade" --if-absent=: 0
  if upgrade-counter > 0:
    upgrade-counter -= 1
    STORAGE["upgrade"] = upgrade-counter
    while true:
      print "There is a newer version of rudelblinken available at this event. Please upgrade."
      print "The device will return to normal operation after $upgrade-counter reboots"
      sleep --ms=5000

require-upgrade:
  STORAGE["upgrade"] = 5
  print "Upgrade required. Rebooting in 5 seconds"
  sleep --ms=5000
  exit 0
