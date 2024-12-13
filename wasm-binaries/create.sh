#!/usr/bin/env bash

NAME="$1"
if [ -z "$NAME" ]; then
    echo "Usage: $0 <name>"
    exit 1
fi
if [ -e "$NAME" ]; then
    echo "Error: $NAME already exists"
    exit 1
fi

set -e
cp -r hello-world "$NAME"
sed -i "s/hello-world/$NAME/g" "$NAME/Cargo.toml"
sed -i 's/"hello-world"/"hello-world", "'"$NAME"'"/g' "./Cargo.toml"
