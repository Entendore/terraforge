use noise::{NoiseFn, Perlin, Simplex, Worley, Seedable};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufWriter, Write};
use image::{ImageBuffer, Rgb, Rgba, Luma};
use imageproc::drawing::draw_line_segment_mut;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

// ==================== TYPES & CONFIG ====================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, clap::ValueEnum, PartialEq)]
pub enum NoiseType { Perlin, Simplex, Ridged, Worley }

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Preset { Default, FantasyIsland, AlienPlanet, DarkFantasy, Mars, Archipelago }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainParams {
    pub width: u32,
    pub height: u32,
    pub seed: u32,
    pub scale: f64,
    pub octaves: usize,
    pub persistence: f64,
    pub lacunarity: f64,
    pub noise_type: NoiseType,
    pub warp_strength: f64,
    pub erosion_particles: usize,
    pub erosion_lifetime: usize,
    pub island_mask_power: f32,
    pub thermal_iterations: usize,
    pub talus_threshold: f32,
}

impl Default for TerrainParams {
    fn default() -> Self {
        Self {
            width: 1024, height: 1024, seed: 1337, scale: 250.0,
            octaves: 6, persistence: 0.5, lacunarity: 2.0,
            noise_type: NoiseType::Perlin, warp_strength: 0.0,
            erosion_particles: 0, erosion_lifetime: 30,
            island_mask_power: 0.0, thermal_iterations: 0, talus_threshold: 0.05,
        }
    }
}

#[derive(Serialize)]
pub struct River {
    pub points: Vec<[f32; 3]>,
    pub length: f32,
}

pub struct TerrainStats {
    pub min: f32, pub max: f32, pub mean: f32, pub std_dev: f32,
    pub avg_slope: f32, pub ruggedness: f32,
}

// ==================== HELPERS ====================

pub fn get_h(hm: &[f32], w: u32, h: u32, x: i32, y: i32) -> f32 {
    let cx = x.clamp(0, w as i32 - 1) as u32;
    let cy = y.clamp(0, h as i32 - 1) as u32;
    hm[(cy * w + cx) as usize]
}

pub fn calc_slope(hm: &[f32], w: u32, h: u32, x: u32, y: u32) -> f32 {
    let gx = (get_h(hm, w, h, x as i32 + 1, y as i32)
            - get_h(hm, w, h, x as i32 - 1, y as i32)) / 2.0;
    let gy = (get_h(hm, w, h, x as i32, y as i32 + 1)
            - get_h(hm, w, h, x as i32, y as i32 - 1)) / 2.0;
    (gx * gx + gy * gy).sqrt()
}

// ==================== GENERATION ====================

pub fn generate_heightmap(params: &TerrainParams) -> Vec<f32> {
    let perlin = Perlin::new().set_seed(params.seed);
    let simplex = Simplex::new().set_seed(params.seed);
    let worley = Worley::new().set_seed(params.seed);
    let warp_noise = Perlin::new().set_seed(params.seed.wrapping_add(999));
    let mut hm = vec![0.0f32; (params.width * params.height) as usize];
    let warp_scale = params.scale * 2.0;

    hm.par_chunks_mut(params.width as usize)
      .enumerate()
      .for_each(|(y, row)| {
        for x in 0..params.width {
            let (wx, wy) = if params.warp_strength > 0.0 {
                let qx = warp_noise.get([x as f64 / warp_scale, y as f64 / warp_scale]);
                let qy = warp_noise.get([
                    x as f64 / warp_scale + 5.2,
                    y as f64 / warp_scale + 1.3,
                ]);
                (qx * params.warp_strength, qy * params.warp_strength)
            } else {
                (0.0, 0.0)
            };

            let mut amp = 1.0f64;
            let mut freq = 1.0f64;
            let mut h = 0.0f64;
            let mut max_a = 0.0f64;

            for _ in 0..params.octaves {
                let sx = (x as f64 + wx) / params.scale * freq;
                let sy = (y as f64 + wy) / params.scale * freq;
                let val = match params.noise_type {
                    NoiseType::Perlin => perlin.get([sx, sy]),
                    NoiseType::Simplex => simplex.get([sx, sy]),
                    NoiseType::Worley => 1.0 - worley.get([sx, sy]),
                    NoiseType::Ridged => 1.0 - perlin.get([sx, sy]).abs(),
                };
                h += val * amp;
                max_a += amp;
                amp *= params.persistence;
                freq *= params.lacunarity;
            }
            row[x as usize] = (h / max_a) as f32;
        }
    });
    hm
}

// ==================== POST-PROCESSING ====================

pub fn apply_island_mask(hm: &mut [f32], p: &TerrainParams) {
    if p.island_mask_power <= 0.0 { return; }
    let cx = p.width as f32 / 2.0;
    let cy = p.height as f32 / 2.0;
    let max_d = cx.min(cy);
    for y in 0..p.height {
        for x in 0..p.width {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt() / max_d;
            let falloff = 1.0 - dist.powf(p.island_mask_power);
            hm[(y * p.width + x) as usize] *= falloff.max(0.0);
        }
    }
}

pub fn apply_thermal_erosion(hm: &mut [f32], p: &TerrainParams) {
    if p.thermal_iterations == 0 { return; }
    let w = p.width;
    let h = p.height;
    let mut temp = vec![0.0f32; (w * h) as usize];

    for _ in 0..p.thermal_iterations {
        temp.copy_from_slice(hm);
        for y in 1..(h - 1) {
            for x in 1..(w - 1) {
                let idx = (y * w + x) as usize;
                let ch = hm[idx];
                let neighbors = [
                    ((y - 1) * w + x) as usize,
                    ((y + 1) * w + x) as usize,
                    (y * w + x - 1) as usize,
                    (y * w + x + 1) as usize,
                ];
                let mut t_diff = 0.0f32;
                let mut diffs = [0.0f32; 4];
                let mut max_d = 0.0f32;
                for (i, &n) in neighbors.iter().enumerate() {
                    let d = ch - hm[n];
                    if d > 0.0 {
                        diffs[i] = d;
                        t_diff += d;
                        if d > max_d { max_d = d; }
                    }
                }
                if max_d > p.talus_threshold && t_diff > 0.0 {
                    let move_amt = (max_d - p.talus_threshold) * 0.5;
                    temp[idx] -= move_amt;
                    for (i, &n) in neighbors.iter().enumerate() {
                        if diffs[i] > 0.0 {
                            temp[n] += (diffs[i] / t_diff) * move_amt;
                        }
                    }
                }
            }
        }
        std::mem::swap(hm, &mut temp);
    }
}

pub fn apply_particle_erosion(hm: &mut [f32], p: &TerrainParams) {
    if p.erosion_particles == 0 { return; }
    let mut rng = StdRng::seed_from_u64(p.seed as u64 + 42);
    let w = p.width;
    let h = p.height;

    for _ in 0..p.erosion_particles {
        let mut px = rng.gen_range(0.0..w as f32);
        let mut py = rng.gen_range(0.0..h as f32);
        let mut dx = 0.0f32;
        let mut dy = 0.0f32;
        let mut speed = 1.0f32;
        let mut water = 1.0f32;
        let mut sed = 0.0f32;

        for _ in 0..p.erosion_lifetime {
            let cx = px as i32;
            let cy = py as i32;
            if cx < 0 || cx >= w as i32 - 1 || cy < 0 || cy >= h as i32 - 1 {
                break;
            }
            let old_h = get_h(hm, w, h, cx, cy);
            let gx = (get_h(hm, w, h, cx + 1, cy) - get_h(hm, w, h, cx - 1, cy)) / 2.0;
            let gy = (get_h(hm, w, h, cx, cy + 1) - get_h(hm, w, h, cx, cy - 1)) / 2.0;
            dx = dx * 0.1 - gx * 0.9;
            dy = dy * 0.1 - gy * 0.9;
            let len = (dx * dx + dy * dy).sqrt();
            if len > 0.0 { dx /= len; dy /= len; }
            px += dx;
            py += dy;
            if px < 0.0 || px >= w as f32 - 1.0 || py < 0.0 || py >= h as f32 - 1.0 {
                break;
            }
            let new_h = get_h(hm, w, h, px as i32, py as i32);
            let dh = new_h - old_h;
            let cap = dh.max(0.0) * speed * water * 4.0;
            let ix = px.clamp(0.0, (w - 1) as f32) as u32;
            let iy = py.clamp(0.0, (h - 1) as f32) as u32;
            let idx = (iy * w + ix) as usize;

            if sed > cap || dh > 0.0 {
                let amt = if dh > 0.0 { dh.min(sed) } else { (sed - cap) * 0.3 };
                sed -= amt;
                hm[idx] += amt;
            } else {
                let amt = ((cap - sed) * 0.3).min(-dh);
                sed += amt;
                hm[idx] -= amt;
            }
            speed = (speed * speed + dh * 4.0).max(0.0).sqrt().min(2.0);
            water *= 0.99;
            if water < 0.01 { break; }
        }
    }
}

pub fn apply_terrace(hm: &mut [f32], steps: u32, smoothness: f32) {
    if steps == 0 { return; }
    let s = steps as f32;
    let sm = smoothness.clamp(0.01, 1.0);
    for val in hm.iter_mut() {
        let n = (*val + 1.0) / 2.0;
        let stepped = (n * s).round() / s;
        let terraced = n + (stepped - n) * (1.0 - sm);
        *val = terraced * 2.0 - 1.0;
    }
}

// ==================== STATISTICS ====================

pub fn get_advanced_statistics(hm: &[f32], p: &TerrainParams) -> TerrainStats {
    let w = p.width;
    let h = p.height;
    let mut min = f32::MAX;
    let mut max = f32::MIN;
    let mut sum = 0.0f32;
    let mut total_slope = 0.0f32;
    let mut rms_sum = 0.0f32;
    let mut count = 0u32;

    for y in 1..(h - 1) {
        for x in 1..(w - 1) {
            let val = hm[(y * w + x) as usize];
            if val < min { min = val; }
            if val > max { max = val; }
            sum += val;
            let dx = hm[(y * w + x + 1) as usize] - hm[(y * w + x - 1) as usize];
            let dy = hm[((y + 1) * w + x) as usize] - hm[((y - 1) * w + x) as usize];
            let ss = dx * dx + dy * dy;
            total_slope += ss.sqrt();
            rms_sum += ss;
            count += 1;
        }
    }
    let mean = sum / (w * h) as f32;
    let var: f32 = hm.iter().map(|&v| (v - mean).powi(2)).sum();
    TerrainStats {
        min, max, mean,
        std_dev: (var / (w * h) as f32).sqrt(),
        avg_slope: total_slope / count.max(1) as f32,
        ruggedness: (rms_sum / count.max(1) as f32).sqrt(),
    }
}

// ==================== COMMERCIAL PRESETS ====================

pub fn apply_preset(p: &mut TerrainParams, preset: &Preset) {
    match preset {
        Preset::Default => {},
        Preset::FantasyIsland => {
            p.noise_type = NoiseType::Simplex;
            p.scale = 300.0;
            p.octaves = 8;
            p.warp_strength = 35.0;
            p.island_mask_power = 2.5;
            p.erosion_particles = 50000;
            p.thermal_iterations = 5;
        },
        Preset::AlienPlanet => {
            p.noise_type = NoiseType::Worley;
            p.scale = 150.0;
            p.octaves = 4;
            p.warp_strength = 80.0;
        },
        Preset::DarkFantasy => {
            p.noise_type = NoiseType::Ridged;
            p.scale = 200.0;
            p.octaves = 7;
            p.warp_strength = 15.0;
            p.erosion_particles = 80000;
            p.thermal_iterations = 10;
            p.talus_threshold = 0.08;
        },
        Preset::Mars => {
            p.noise_type = NoiseType::Ridged;
            p.scale = 400.0;
            p.octaves = 6;
            p.warp_strength = 10.0;
        },
        Preset::Archipelago => {
            p.noise_type = NoiseType::Simplex;
            p.scale = 150.0;
            p.octaves = 6;
            p.warp_strength = 50.0;
            p.island_mask_power = 1.2;
            p.erosion_particles = 30000;
        },
    }
}

// ==================== EXPORTERS ====================

pub fn export_to_png(hm: &[f32], p: &TerrainParams, path: &str) {
    let img = ImageBuffer::from_fn(p.width, p.height, |x, y| {
        let val = hm[(y * p.width + x) as usize];
        Luma([((val + 1.0) / 2.0 * 65535.0) as u16])
    });
    img.save(path).expect("Failed to save PNG");
}

pub fn export_normal_map(hm: &[f32], p: &TerrainParams, path: &str, strength: f32) {
    let w = p.width;
    let h = p.height;
    let img = ImageBuffer::from_fn(w, h, |x, y| {
        let dx = (get_h(hm, w, h, x as i32 + 1, y as i32)
                - get_h(hm, w, h, x as i32 - 1, y as i32)) * strength;
        let dy = (get_h(hm, w, h, x as i32, y as i32 + 1)
                - get_h(hm, w, h, x as i32, y as i32 - 1)) * strength;
        let len = (dx * dx + dy * dy + 1.0).sqrt();
        Rgb([
            ((-dx / len * 0.5 + 0.5) * 255.0) as u8,
            ((-dy / len * 0.5 + 0.5) * 255.0) as u8,
            ((1.0 / len * 0.5 + 0.5) * 255.0) as u8,
        ])
    });
    img.save(path).expect("Failed to save normal map");
}

pub fn export_splatmap(hm: &[f32], p: &TerrainParams, path: &str) {
    let w = p.width;
    let h = p.height;
    let img = ImageBuffer::from_fn(w, h, |x, y| {
        let hv = hm[(y * w + x) as usize];
        let slope = calc_slope(hm, w, h, x, y);
        let (mut g, mut r, mut s, mut sa) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        if hv < -0.05 {
            sa = 1.0;
        } else if hv < 0.05 {
            let t = (hv + 0.05) / 0.1;
            sa = 1.0 - t; g = t;
        } else if slope > 0.12 {
            let t = ((slope - 0.12) / 0.08).min(1.0);
            g = 1.0 - t; r = t;
        } else if hv > 0.5 {
            let t = ((hv - 0.5) / 0.2).min(1.0);
            g = 1.0 - t; s = t;
        } else {
            g = 1.0;
        }
        let tot = (g + r + s + sa).max(0.001);
        Rgba([
            (g / tot * 255.0) as u8,
            (r / tot * 255.0) as u8,
            (s / tot * 255.0) as u8,
            (sa / tot * 255.0) as u8,
        ])
    });
    img.save(path).expect("Failed to save splatmap");
}

pub fn export_albedo(hm: &[f32], p: &TerrainParams, path: &str) {
    let w = p.width;
    let h = p.height;
    let img = ImageBuffer::from_fn(w, h, |x, y| {
        let hv = hm[(y * w + x) as usize];
        let slope = calc_slope(hm, w, h, x, y);
        let c = if hv < -0.1 { [40, 90, 160] }
            else if hv < 0.0 { [194, 178, 128] }
            else if slope > 0.15 { [100, 95, 90] }
            else if hv > 0.6 { [240, 245, 250] }
            else { [70, 130, 50] };
        Rgb(c)
    });
    img.save(path).expect("Failed to save albedo");
}

pub fn export_thumbnail(hm: &[f32], p: &TerrainParams, path: &str, size: u32) {
    let w = p.width;
    let h = p.height;
    let scale_x = w as f32 / size as f32;
    let scale_y = h as f32 / size as f32;
    let img = ImageBuffer::from_fn(size, size, |x, y| {
        let sx = (x as f32 * scale_x) as u32;
        let sy = (y as f32 * scale_y) as u32;
        let hv = hm[(sy * w + sx) as usize];
        let c = if hv < -0.1 { [40, 90, 160] }
            else if hv < 0.0 { [194, 178, 128] }
            else if hv < 0.3 { [70, 130, 50] }
            else if hv < 0.6 { [100, 95, 90] }
            else { [240, 245, 250] };
        Rgb(c)
    });
    img.save(path).expect("Failed to save thumbnail");
}

// ==================== FANTASY MAP & RIVERS ====================

pub fn extract_rivers(hm: &[f32], p: &TerrainParams, count: usize) -> Vec<River> {
    let mut rng = StdRng::seed_from_u64(p.seed as u64 + 1234);
    let w = p.width;
    let h = p.height;
    let mut rivers = Vec::new();
    let mut attempts = 0;
    while rivers.len() < count && attempts < count * 50 {
        attempts += 1;
        let mut x = rng.gen_range(10..(w.saturating_sub(10).max(11))) as i32;
        let mut y = rng.gen_range(10..(h.saturating_sub(10).max(11))) as i32;
        if hm[(y as u32 * w + x as u32) as usize] < 0.4 { continue; }
        let mut pts = Vec::new();
        let mut len = 0.0f32;
        for _ in 0..(w + h) {
            let hv = hm[(y as u32 * w + x as u32) as usize];
            pts.push([x as f32, hv * 150.0, y as f32]);
            let mut min_h = hv;
            let mut nx = x;
            let mut ny = y;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 { continue; }
                    let cx = x + dx;
                    let cy = y + dy;
                    if cx < 0 || cx >= w as i32 || cy < 0 || cy >= h as i32 { continue; }
                    let nh = hm[(cy as u32 * w + cx as u32) as usize];
                    if nh < min_h {
                        min_h = nh;
                        nx = cx;
                        ny = cy;
                    }
                }
            }
            if nx == x && ny == y { break; }
            len += ((nx - x).pow(2) as f32 + (ny - y).pow(2) as f32).sqrt();
            x = nx;
            y = ny;
            if hv < -0.1 { break; }
        }
        if pts.len() > 10 {
            rivers.push(River { points: pts, length: len });
        }
    }
    rivers
}

pub fn export_fantasy_map(hm: &[f32], p: &TerrainParams, rivers: &[River], path: &str) {
    let w = p.width;
    let h = p.height;
    let mut map = ImageBuffer::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let hv = hm[(y * w + x) as usize];
            let c = if hv < -0.05 { [60, 140, 200] }
                else if hv < 0.0 { [100, 180, 220] }
                else if hv < 0.2 { [140, 190, 90] }
                else if hv < 0.5 { [90, 150, 60] }
                else if hv < 0.7 { [140, 130, 110] }
                else { [230, 230, 240] };
            map.put_pixel(x, y, Rgb(c));
        }
    }
    for r in rivers {
        for win in r.points.windows(2) {
            draw_line_segment_mut(
                &mut map,
                (win[0][0], win[0][2]),
                (win[1][0], win[1][2]),
                Rgb([30, 80, 180]),
            );
        }
    }
    map.save(path).expect("Failed to save fantasy map");
}

// ==================== SEED EXPLORER ====================

pub fn generate_contact_sheet(
    base_params: &TerrainParams,
    grid_size: u32,
    thumb_size: u32,
    out_dir: &str,
    on_progress: impl Fn(u32, u32),
) {
    let mut sheet = ImageBuffer::new(grid_size * thumb_size, grid_size * thumb_size);
    let total = grid_size * grid_size;
    for i in 0..total {
        let seed = base_params.seed.wrapping_add(i);
        let mut p = base_params.clone();
        p.seed = seed;
        p.width = thumb_size;
        p.height = thumb_size;
        let mut hm = generate_heightmap(&p);
        if p.island_mask_power > 0.0 { apply_island_mask(&mut hm, &p); }
        if p.erosion_particles > 0 { apply_particle_erosion(&mut hm, &p); }
        if p.thermal_iterations > 0 { apply_thermal_erosion(&mut hm, &p); }

        let col = i % grid_size;
        let row = i / grid_size;
        let ox = col * thumb_size;
        let oy = row * thumb_size;

        for y in 0..thumb_size {
            for x in 0..thumb_size {
                let hv = hm[(y * thumb_size + x) as usize];
                let c = if hv < -0.1 { [40, 90, 160] }
                    else if hv < 0.0 { [194, 178, 128] }
                    else if hv < 0.3 { [70, 130, 50] }
                    else if hv < 0.6 { [100, 95, 90] }
                    else { [240, 245, 250] };
                sheet.put_pixel(ox + x, oy + y, Rgb(c));
            }
        }
        on_progress(i + 1, total);
    }
    let path = format!("{}/seed_explorer_grid.png", out_dir);
    sheet.save(&path).expect("Failed to save contact sheet");
}

// ==================== README GENERATOR ====================

pub fn generate_readme(out_dir: &str, p: &TerrainParams, is_demo: bool) {
    let demo = if is_demo {
        "\n> **DEMO MODE**: Resolution capped at 512x512. Purchase the full version to unlock 4K+ resolution and all engine exporters!"
    } else { "" };
    let text = format!(
r#"# TerraForge Terrain Export

Generated with **TerraForge v1.0.0**
Seed: `{}` | Size: {}x{}
{}

## 📁 File Guide
- `heightmap_{}.png` — 16-bit grayscale heightmap. Import into your engine's terrain tool.
- `splatmap_{}.png` — RGBA texture mask. R=Grass, G=Rock, B=Snow, A=Sand.
- `normal_{}.png` — Baked normal map for distant LODs.
- `albedo_{}.png` — Procedural diffuse texture.
- `params_{}.json` — Exact parameters used. Keep this for reproducibility!

## 🎮 Engine Import Tips
- **Unity**: Use the built-in Terrain tool. Import the 16-bit PNG as the heightmap.
- **Godot**: Use the [Terrain3D](https://github.com/TokisanGames/Terrain3D) plugin.
- **Unreal Engine 5**: Use Landscape mode. Import the 16-bit RAW/PNG.

---
*Made with TerraForge. Get the full version at: https://yourstudio.itch.io/terraforge*
"#,
        p.seed, p.width, p.height, demo,
        p.seed, p.seed, p.seed, p.seed, p.seed,
    );
    std::fs::write(format!("{}/README.md", out_dir), text).expect("Failed to write README");

    // Also save params JSON for reproducibility
    let json = serde_json::to_string_pretty(p).expect("Failed to serialize params");
    std::fs::write(format!("{}/params_{}.json", out_dir, p.seed), json)
        .expect("Failed to write params JSON");
}

// ==================== UNIT TESTS ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_generation() {
        let p = TerrainParams {
            width: 64, height: 64, seed: 42, ..Default::default()
        };
        let map1 = generate_heightmap(&p);
        let map2 = generate_heightmap(&p);
        assert_eq!(map1, map2, "Heightmap generation is not deterministic!");
    }

    #[test]
    fn test_island_mask_application() {
        let mut p = TerrainParams {
            width: 100, height: 100, seed: 1, island_mask_power: 3.0, ..Default::default()
        };
        let mut hm = generate_heightmap(&p);
        let center_before = hm[(50 * 100 + 50) as usize];
        let edge_before = hm[(0 * 100 + 0) as usize];
        apply_island_mask(&mut hm, &p);
        let edge_after = hm[(0 * 100 + 0) as usize];
        assert!(edge_after.abs() <= edge_before.abs() + 0.001,
            "Edge was not flattened by island mask");
        let _ = center_before;
    }

    #[test]
    fn test_advanced_statistics() {
        let p = TerrainParams { width: 10, height: 10, seed: 1, ..Default::default() };
        let hm = vec![0.0, 0.5, -0.5, 1.0, -1.0, 0.2, -0.2, 0.8, -0.8, 0.0,
                      0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0,
                      -0.1, -0.2, -0.3, -0.4, -0.5, -0.6, -0.7, -0.8, -0.9, -1.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                      0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let stats = get_advanced_statistics(&hm, &p);
        assert!((stats.min - (-1.0)).abs() < 0.01, "Min calculation failed");
        assert!((stats.max - 1.0).abs() < 0.01, "Max calculation failed");
        assert!(stats.mean >= stats.min && stats.mean <= stats.max, "Mean out of bounds");
        assert!(stats.std_dev >= 0.0, "StdDev cannot be negative");
    }

    #[test]
    fn test_preset_application() {
        let mut p = TerrainParams::default();
        apply_preset(&mut p, &Preset::FantasyIsland);
        assert_eq!(p.noise_type, NoiseType::Simplex);
        assert!(p.island_mask_power > 0.0, "FantasyIsland should have island mask");
        assert!(p.erosion_particles > 0, "FantasyIsland should have erosion");
    }

    #[test]
    fn test_thermal_erosion_stability() {
        let mut p = TerrainParams {
            width: 32, height: 32, seed: 99,
            thermal_iterations: 5, talus_threshold: 0.1,
            ..Default::default()
        };
        let mut hm = generate_heightmap(&p);
        let orig_min = hm.iter().cloned().fold(f32::MAX, f32::min);
        let orig_max = hm.iter().cloned().fold(f32::MIN, f32::max);
        apply_thermal_erosion(&mut hm, &p);
        let new_min = hm.iter().cloned().fold(f32::MAX, f32::min);
        let new_max = hm.iter().cloned().fold(f32::MIN, f32::max);
        assert!(new_min >= orig_min - 0.2, "Thermal erosion created unnatural pits");
        assert!(new_max <= orig_max + 0.2, "Thermal erosion created unnatural peaks");
    }

    #[test]
    fn test_terrace_preserves_range() {
        let mut hm = vec![0.0f32; 100];
        for (i, v) in hm.iter_mut().enumerate() {
            *v = (i as f32 / 100.0) * 2.0 - 1.0;
        }
        apply_terrace(&mut hm, 10, 0.5);
        for &v in &hm {
            assert!(v >= -1.0 && v <= 1.0, "Terrace pushed value out of range");
        }
    }
}
