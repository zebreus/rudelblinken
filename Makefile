install-all:
	jag scan -t 8000ms -o short --list | xargs -n1 jag container install rudelblinken main.toit -d

uninstall-all:
	jag scan -t 4000ms -o short --list | xargs -n1 jag container uninstall rudelblinken -d

run-all:
	jag scan -t 4000ms -o short --list | xargs -n1 jag run main.toit -d

start-ap:
	sudo create_ap --freq-band 2.4 -c 6 $$(ip link | grep -Po '^[0-9]+: [^ :]+' | grep -Po 'w[^ ]+$$' ) $$(ip route get 1.1.1.1 | grep -Po '(?<=(dev ))(\S+)')  'rudelctrl' '22po7gl334ai'

esp32c3-firmware.envelope:
	nix run github:zebreus/toit-envelope-with-espnow

flash-%: esp32c3-firmware.envelope
	jag flash esp32c3-firmware.envelope --chip esp32c3 --wifi-ssid rudelctrl --wifi-password 22po7gl334ai --name $*