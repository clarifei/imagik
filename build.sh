#!/bin/bash

# build script with optimizations
# usage: ./build.sh [release|debug|native|deploy]

set -e

MODE=${1:-release}

echo "building imagik - image transformation server"
echo "mode: $MODE"
echo ""

case "$MODE" in
  release)
    echo "building optimized release binary..."
    echo "   - level 3 optimizations"
    echo "   - fast resize with SIMD (AVX2/SSE4.1)"
    echo "   - lto enabled"
    echo "   - strip symbols"
    cargo build --release
    echo ""
    echo "binary size:"
    ls -lh target/release/imagik
    ;;
    
  native)
    echo "building with native cpu optimizations (avx2, etc.)..."
    echo "   - auto-detects your cpu features"
    echo "   - maximum performance on this machine"
    echo "   - enables fast_image_resize SIMD (AVX2/SSE4.1)"
    echo "   - warning: binary only works on this cpu!"
    RUSTFLAGS="-C target-cpu=native" cargo build --release
    echo ""
    echo "binary optimized for your cpu"
    ls -lh target/release/imagik
    ;;
    
  deploy)
    echo "building portable release (no cpu-specific optimizations)..."
    echo "   - works on any x86_64 cpu"
    echo "   - slightly larger but maximum compatibility"
    rm -f .cargo/config.toml  # remove native cpu flags
    cargo build --release
    echo ""
    echo "portable binary ready for deployment"
    ls -lh target/release/imagik
    ;;
    
  debug)
    echo "building debug version..."
    cargo build
    ;;
    
  *)
    echo "usage: $0 [release|native|deploy|debug]"
    echo ""
    echo "modes:"
    echo "  release - standard optimized build"
    echo "  native  - optimized for this cpu (avx2, etc.) - fastest"
    echo "  deploy  - portable build for any cpu"
    echo "  debug   - debug build with symbols"
    exit 1
    ;;
esac
