# Get the name of the primary output
PRIMARY_OUTPUT=$(xrandr --query | grep " primary" | cut -d" " -f1)

# Check if the primary output was found
if [ -z "$PRIMARY_OUTPUT" ]; then
  echo "No primary output found"
  exit 1
fi
