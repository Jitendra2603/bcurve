#!/bin/bash

# DLMM Bonding Curve 
echo "======================================"
echo "DLMM Bonding Curve Setup"
echo "======================================"

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "Rust is not installed. Installing Rust..."
    echo ""
    echo "This will install the Rust toolchain via rustup."
    echo "Press Enter to continue or Ctrl-C to cancel..."
    read
    
    # Install Rust
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    
    # Source the cargo env
    source $HOME/.cargo/env
    
    echo "Rust installed successfully!"
else
    echo "Rust is already installed ✓"
fi

echo ""
echo "Building the project..."
echo ""

cargo build --release

if [ $? -eq 0 ]; then
    echo ""
    echo "✓ Build successful!"
    echo ""
    echo "You can now run the verifier with:"
    echo "  ./target/release/bcurve --help"
    echo ""
    echo "Example usage:"
    echo "  ./target/release/bcurve \\"
    echo "    --mode geometric \\"
    echo "    --p0 0.01 --bin-step-bps 10 \\"
    echo "    --theta 0.6 \\"
    echo "    --target-supply 100000000 \\"
    echo "    --end-price 0.0175 \\"
    echo "    --verbose true"
else
    echo ""
    echo "❌ Build failed. Please check the error messages above."
fi
