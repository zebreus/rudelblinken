// Copyright (C) 2023 Toitware ApS.
// Use of this source code is governed by a Zero-Clause BSD license that can
// be found in the examples/LICENSE file.

import system


main:
  print "hi"
  catch --trace:
    throw "he"

  print "ho"

  while true:
    sleep --ms=200
    print "ho"