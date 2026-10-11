# 🏔️ TerraForge v1.0.0

**Professional procedural terrain generator for games and TTRPGs.**

TerraForge is a production-ready CLI tool that generates beautiful, export-ready landscapes in seconds. Perfect for indie game developers, TTRPG GMs, and academic researchers.

## ✨ Features

- **Curated Presets**: One-click generation for Fantasy Island, Alien Planet, Dark Fantasy, Mars, and Archipelago
- **Advanced Simulations**: Hydraulic erosion, thermal erosion, domain warping, island masks
- **Fantasy Map Mode**: Exports stylized maps with rivers — perfect for TTRPGs
- **Seed Explorer**: Generate a visual grid of seeds to find the perfect landscape
- **Engine-Ready Exports**: 16-bit heightmaps, splatmaps, normal maps, albedo textures
- **Demo Mode**: Free tier with 512x512 cap for itch.io distribution

## 🚀 Quick Start

```bash
# Build
cargo build --release

# Generate a default terrain
./target/release/terraforge --seed 42

# Use a preset
./target/release/terraforge --preset fantasy-island --seed 42

# Generate a fantasy map with rivers
./target/release/terraforge --preset fantasy-island --fantasy-map --seed 42

# Explore seeds visually (4x4 grid)
./target/release/terraforge --explore 4 --seed 100

# Demo mode (512x512 cap)
./target/release/terraforge --demo --seed 42
