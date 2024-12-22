#!/bin/bash

# Infinite loop to monitor network and reconnect if necessary
while true; do
    # Check connectivity by pinging a reliable public server
    if ! ping -c 1 -W 2 8.8.8.8 &> /dev/null; then
        echo "$(date): Network unreachable. Restarting Wi-Fi..."
        nmcli radio wifi off
        sleep 2
        nmcli radio wifi on
    fi
    # Sleep before checking again to avoid excessive resource usage
    sleep 10
done

