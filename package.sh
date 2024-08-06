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

# Function to find and copy dependencies
find_and_copy_dependencies() {
    local file=$1
    local dependencies=$(ldd "$file" | grep '=>' | awk '{print $3}' | sort -u)

    for dep in $dependencies; {
        # Only copy if the dependency exists and is not already in the destination
        if [ -e "$dep" ] && [ ! -e "$(basename "$dep")" ]; then
            cp -v "$dep" .
            # Recursively find dependencies of the copied library
            find_and_copy_dependencies "$(basename "$dep")"
        fi
    }
}

# Start by copying the dependencies of the main executable
find_and_copy_dependencies "$EXECUTABLE_BASENAME"

# Use patchelf to set the RPATH of the executable and all the copied libraries
for file in *; do
    if file "$file" | grep -q 'ELF'; then
        patchelf --set-rpath '$ORIGIN' "$file"
    fi
done

echo "Executable and its dependencies have been copied to $DESTINATION"
echo "RPATH has been set to look for libraries in its own directory"
