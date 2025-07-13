#!/bin/bash

# Initialize variables
REMOTE_HOST=""
FORCE=false

# Function to display help message
show_help() {
    echo "Usage: $0 [OPTIONS] HOST"
    echo
    echo "Positional argument:"
    echo "  HOST        The host to target"
    echo
    echo "Optional arguments:"
    echo "  --force     Set the FORCE flag to true"
    echo "  -h, --help  Show this help message"
    exit 0
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help)
            show_help
            ;;
        --force)
            FORCE=true
            shift
            ;;
        *)
            if [[ -z "$REMOTE_HOST" ]]; then
                REMOTE_HOST="$1"
            else
                echo "Error: Unknown argument $1"
                show_help
            fi
            shift
            ;;
    esac
done

# Check if the REMOTE_HOST is provided
if [[ -z "$REMOTE_HOST" ]]; then
    echo "Error: HOST is required."
    show_help
fi

if $FORCE; then
    echo "Force mode enabled. All files will be pushed."
else
    echo "Normal mode. Only new or modified files will be pushed."
fi

TARGET="armv7-unknown-linux-gnueabihf"
REMOTE_TARGET=$(ssh $REMOTE_HOST uname -m)
if [ $? -eq 0 ]; then
    if [ $REMOTE_TARGET = "aarch64" ]; then
        echo "Remote system is ARM64."
        TARGET="aarch64-unknown-linux-gnu"
    fi
fi
cross build --target $TARGET --release
ssh $REMOTE_HOST -t 'mkdir -p ~/thermo-server'
ssh -q $REMOTE_HOST -t 'stat "thermo-server/thermo-server" &> /dev/null'
if [ $? -ne 0 ] || $FORCE; then
    scp target/$TARGET/release/thermo-server $REMOTE_HOST:~/thermo-server
else
    echo "Not updating server binary."
fi
scp target/$TARGET/release/thermo-tester $REMOTE_HOST:~/thermo-server
scp target/$TARGET/release/humi-tester $REMOTE_HOST:~/thermo-server
scp target/$TARGET/release/thermo-ident $REMOTE_HOST:~/thermo-server
scp target/$TARGET/release/thermo-cputemp $REMOTE_HOST:~/thermo-server
ssh -q $REMOTE_HOST -t 'stat "thermo-server/thermo.env" &> /dev/null'
if [ $? -ne 0 ]; then
    scp thermo.env thermo.service $REMOTE_HOST:~/thermo-server
fi
scp disable_ethernet_gadget.sh $REMOTE_HOST:~/thermo-server
