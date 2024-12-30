class PeerInformation:
  estimated-duration := 0
  estimated-shift := 0

ESTIMATION-WINDOW ::= 1000 * 1000 * 10

class Peer:
  received-pings /List := []

  receive-ping at:
    received-pings.add at
  
  analyze-peer at:
    received-pings.filter --in-place=true:
      it > (at - ESTIMATION-WINDOW)
    received-pings.sort
    recent-pings := received-pings.filter --in-place=false:
      it <= at

    if recent-pings.size < 2:
        return null

    durations := []
    smallest-duration := int.MAX-U32
    average-duration := 0
    for i := 0; i < recent-pings.size - 1; i++:
      duration := recent-pings[i + 1] - recent-pings[i];
      durations.add duration
      average-duration += duration
      if duration < smallest-duration:
        smallest-duration = duration
    durations.sort
    median-duration := durations[durations.size / 2]
    average-duration /= recent-pings.size - 1

    estimated-duration := median-duration
    // estimated-duration := smallest-duration

    estimated-shift := estimated-duration - ((estimated-duration + ((at - recent-pings.last) % estimated-duration)) % estimated-duration)

    peer-information := PeerInformation
    peer-information.estimated-duration = estimated-duration
    peer-information.estimated-shift = estimated-shift
    return peer-information

class LamePeer:
  received-pings /List := []
  preferred-frequency := 0

  receive-ping at preferred-duration:
    received-pings.add at
    preferred-frequency = preferred-duration
  
  analyze-peer at:
    received-pings.filter --in-place=true:
      it > (at - ESTIMATION-WINDOW)
    received-pings.sort
    recent-pings := received-pings.filter --in-place=false:
      it <= at

    if recent-pings.size < 2:
        return null

    estimated-duration := preferred-frequency

    estimated-shift := estimated-duration - ((estimated-duration + ((at - recent-pings.last) % estimated-duration)) % estimated-duration)

    peer-information := PeerInformation
    peer-information.estimated-duration = estimated-duration
    peer-information.estimated-shift = estimated-shift
    return peer-information

peers /Map := {:}

receive-ping from/string at/int preferred-duration/int:
  peer := peers.get from --init=: LamePeer
  peer.receive-ping at preferred-duration

predict-next-duration preferred/PeerInformation last/PeerInformation at/int:
  information /List := peers.values.map:
    it.analyze-peer at
  information.filter --in-place=true :
    it != null
  
  average-offset /float := 0.0
  average-offset += 0.05 * preferred.estimated-duration;
  average-offset += last.estimated-duration;
  information.do:
    // print "PEER: Duration: $(it.estimated-duration) Offset: $(it.estimated-shift)"
    average-offset += it.estimated-shift;
    if it.estimated-shift < (it.estimated-duration / 2):
      average-offset += it.estimated-duration
  average-offset /= information.size + 1.05;

  print "Average $average-offset"
  next-cycle-length := average-offset
  return next-cycle-length

