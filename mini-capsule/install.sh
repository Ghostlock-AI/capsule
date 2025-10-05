#!/bin/bash

set -e

echo "Installing mini-capsule..."
cargo install --path . --force
echo "Installation complete! You can now use 'minic' command."