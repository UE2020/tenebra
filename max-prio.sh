#!/bin/bash
PID=$(pidof tenebra)
sudo renice -20 -p "$PID"
sudo ionice -c1 -n0 -p "$PID"
for tid in /proc/$PID/task/*; do
    sudo chrt -f -p 99 "$(basename "$tid")"
done
