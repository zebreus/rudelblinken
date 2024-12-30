if ! test -e "names.txt"; then
    echo "'names.txt' file not found"
    exit 1
fi

function random-two-digits {
    echo $((RANDOM % 100))
}

function flash-device {
    name="$1-$(random-two-digits)"
    echo "Flashing device $name"
    DEVICE=$(echo /dev/tty* | grep -Po '/dev/ttyACM[0-9]' | cut -d ' ' -f 1)
    echo "Device: $DEVICE"
    while :; do
        while test -z "$DEVICE"; do
            echo "Waiting for device to connect"
            sleep 1
            DEVICE=$(echo /dev/tty* | grep -Po '/dev/ttyACM[0-9]' | cut -d ' ' -f 1)
        done
        jag flash esp32c3-firmware.envelope --port "$DEVICE" --chip esp32c3 --wifi-ssid rudelctrl --wifi-password 22po7gl334ai --name "$name"
        if test "$?" -eq 0; then
            echo "Flashed device $name"
            break
        fi
    done
    while test -e "$DEVICE"; do
        echo "Waiting for $DEVICE to disconnect"
        sleep 1
    done
    echo "Flashed with name $name"
    # Flash device
}

while read -r name; do
    flash-device "$name"
done <names.txt
