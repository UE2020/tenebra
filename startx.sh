#!/bin/bash

export DISPLAY=:1

sudo X :1 -config xorg.conf &
sleep 2

cd ../../
startxfce4 &
sleep 2

cd Documents/tenebra
sudo ./target/release/tenebra

