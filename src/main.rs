use clap::Parser;
use std::fs;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use terraforge::*;

#[derive(Parser, Debug)]
#[command(
    name = "TerraForge",
    version = "1.0.0",
    about = "🏔️ Professional Procedural Terrain Generator",
    long_about = "TerraForge is a production-ready terrain generator for game developers, TTRPG creators, and researchers."
)]
struct Args {
    #[arg(short, long, default_value_t = 1024)] width: u32,
    #[arg(short, long, default_value_t = 1024)] height: u32,
    #[arg(short, long, default_value_t = 1337)] seed: u32,

    #[arg(long, value_enum, help = "Use a curated preset (overrides other params)")]
    preset: Option<Preset>,

    #[arg(long, default_value_t = 250.0)] scale: f64,
    #[arg(long, default_value_t = 6)] octaves: usize,
    #[arg(long, value_enum, default_value_t = NoiseType::Perlin)] noise_type: NoiseType,
    #[arg(long, default_value_t = 0.0)] warp_strength: f64,
    #[arg(long, default_value_t = 0.0)] island_mask_power: f32,
    #[arg(long, default_value_t = 0)] erosion_particles: usize,
    #[arg(long, default_value_t = 30)] erosion_lifetime: usize,
    #[arg(long, default_value_t = 0)] thermal_iterations: usize,
    #[arg(long, default_value_t = 0.05)] talus_threshold: f32,

    #[arg(long, default_value_t = false, help = "Export stylized fantasy map with rivers")]
    fantasy_map: bool,
    #[arg(long, default_value_t = false, help = "Export albedo texture")]
    albedo: bool,
    #[arg(long, default_value_t = false, help = "Export normal map")]
    normal_map: bool,
    #[arg(long, default_value_t = false, help = "Export splatmap")]
    splatmap: bool,
    #[arg(long, default_value_t = false, help = "Generate preview thumbnail")]
    thumbnail: bool,
    #[arg(long, default_value_t = 256)] thumbnail_size: u32,

    #[arg(long, value_parser = clap::value_parser!(u32).range(2..=8),
          help = "Generate a NxN grid of seed thumbnails")]
    explore: Option<u32>,

    #[arg(long, default_value_t = false, help = "Enable demo mode (512x512 max, watermark)")]
    demo: bool,

    #[arg(short, long, default_value = "./output")] out_dir: String,
}

fn main() {
    let mut args = Args::parse();

    if args.demo {
        args.width = args.width.min(512);
        args.height = args.height.min(512);
        println!(
            "{}",
            style("⚠ Running in DEMO mode. Resolution capped at 512x512. Purchase the full version on itch.io for 4K+ exports!")
                .yellow()
                .bold()
        );
    }

    let mut params = TerrainParams {
        width: args.width,
        height: args.height,
        seed: args.seed,
        scale: args.scale,
        octaves: args.octaves,
        persistence: 0.5,
        lacunarity: 2.0,
        noise_type: args.noise_type,
        warp_strength: args.warp_strength,
        erosion_particles: args.erosion_particles,
        erosion_lifetime: args.erosion_lifetime,
        island_mask_power: args.island_mask_power,
        thermal_iterations: args.thermal_iterations,
        talus_threshold: args.talus_threshold,
    };

    if let Some(preset) = &args.preset {
        println!("{} Applying preset: {}", style("✔").green(), style(format!("{:?}", preset)).cyan().bold());
        apply_preset(&mut params, preset);
    }

    fs::create_dir_all(&args.out_dir).expect("Failed to create output directory");

    // Handle Seed Explorer Mode
    if let Some(grid) = args.explore {
        println!("{} Exploring seeds ({}x{} grid)...", style("🔍").blue(), grid, grid);
        let pb = ProgressBar::new((grid * grid) as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} seeds explored")
                .unwrap(),
        );
        generate_contact_sheet(&params, grid, 256, &args.out_dir, |_, _| pb.inc(1));
        pb.finish_with_message("Exploration complete!");
        println!("{} Saved contact sheet to {}/seed_explorer_grid.png", style("✔").green(), args.out_dir);
        return;
    }

    println!("{} Initializing TerraForge v1.0.0...", style("🏔️").blue());
    println!("   Seed: {} | Size: {}x{} | Noise: {:?}",
        params.seed, params.width, params.height, params.noise_type);

    let pb = ProgressBar::new(4);
    pb.set_style(ProgressStyle::default_bar().template("{spinner:.green} {msg}").unwrap());

    pb.set_message("Generating base noise...");
    let mut hm = generate_heightmap(&params);
    pb.inc(1);

    pb.set_message("Applying simulations...");
    if params.island_mask_power > 0.0 { apply_island_mask(&mut hm, &params); }
    if params.erosion_particles > 0 { apply_particle_erosion(&mut hm, &params); }
    if params.thermal_iterations > 0 { apply_thermal_erosion(&mut hm, &params); }
    pb.inc(1);

    pb.set_message("Computing statistics...");
    let stats = get_advanced_statistics(&hm, &params);
    pb.inc(1);

    pb.set_message("Exporting assets...");
    let hm_path = format!("{}/heightmap_{}.png", args.out_dir, params.seed);
    export_to_png(&hm, &params, &hm_path);
    println!("   {} Heightmap: {}", style("✔").green(), hm_path);

    if args.normal_map {
        let p = format!("{}/normal_{}.png", args.out_dir, params.seed);
        export_normal_map(&hm, &params, &p, 2.0);
        println!("   {} Normal Map: {}", style("✔").green(), p);
    }
    if args.splatmap {
        let p = format!("{}/splatmap_{}.png", args.out_dir, params.seed);
        export_splatmap(&hm, &params, &p);
        println!("   {} Splatmap: {}", style("✔").green(), p);
    }
    if args.albedo {
        let p = format!("{}/albedo_{}.png", args.out_dir, params.seed);
        export_albedo(&hm, &params, &p);
        println!("   {} Albedo: {}", style("✔").green(), p);
    }
    if args.thumbnail {
        let p = format!("{}/thumbnail_{}.png", args.out_dir, params.seed);
        export_thumbnail(&hm, &params, &p, args.thumbnail_size);
        println!("   {} Thumbnail: {}", style("✔").green(), p);
    }
    if args.fantasy_map {
        println!("   {} Extracting river networks...", style("🌊").blue());
        let rivers = extract_rivers(&hm, &params, 20);
        let p = format!("{}/fantasy_map_{}.png", args.out_dir, params.seed);
        export_fantasy_map(&hm, &params, &rivers, &p);
        println!("   {} Fantasy Map: {}", style("✔").green(), p);
    }

    generate_readme(&args.out_dir, &params, args.demo);
    pb.finish_with_message("Complete!");

    println!("\n📊 Statistics:");
    println!("   Elevation  → Min: {:.3}  Max: {:.3}  Mean: {:.3}  StdDev: {:.3}",
        stats.min, stats.max, stats.mean, stats.std_dev);
    println!("   Geometry   → Avg Slope: {:.4}  Ruggedness: {:.4}",
        stats.avg_slope, stats.ruggedness);

    println!("\n{} TerraForge generation complete! Happy developing.", style("🎉").green().bold());
    if args.demo {
        println!("{} Unlock 4K resolution and all features at: https://yourstudio.itch.io/terraforge",
            style("🔓").yellow());
    }
}
