#!/bin/bash

# Check if the correct number of arguments are provided
if [ "$#" -ne 2 ]; then
    echo "Usage: $0 <path_to_executable> <destination_folder>"
    exit 1
fi

EXECUTABLE=$1
DESTINATION=$2

# Create the destination folder if it doesn't exist
mkdir -p "$DESTINATION"

# Copy the executable to the destination folder
cp "$EXECUTABLE" "$DESTINATION"

# Get the basename of the executable
EXECUTABLE_BASENAME=$(basename "$EXECUTABLE")

# Change to the destination folder
cd "$DESTINATION" || exit

# Use ldd to list the dependencies and copy them to the destination folder
ldd "$EXECUTABLE_BASENAME" | grep '=>' | awk '{print $3}' | xargs -I '{}' cp -v '{}' .

# Use ldd to also handle dependencies that are not explicitly linked with '=>'
ldd "$EXECUTABLE_BASENAME" | grep -o '/lib.*\.so.[0-9]*' | xargs -I '{}' cp -v '{}' .

# Change the RPATH of the executable to look for libraries in its own directory
patchelf --set-rpath '$ORIGIN' "$EXECUTABLE_BASENAME"

echo "Executable and its dependencies have been copied to $DESTINATION"
echo "Executable's RPATH has been set to look for libraries in its own directory"
