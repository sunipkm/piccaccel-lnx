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

# ssh $REMOTE_HOST date -s @$(date -u +"%s")

rsync -vhra . $REMOTE_HOST:~/piccaccel-lnx/ --include='**.gitignore' --exclude='/.git' --filter=':- .gitignore' --delete-after