#!/usr/bin/env node
// assemble-gifs.js — Convert SVG frames + timing into animated GIFs.
//
// Usage: node scripts/assemble-gifs.js [name]
//   Reads: target/animations/{name}/frame-*.svg + timing.json
//   Writes: target/animations/{name}.gif
//
// If no name given, processes all animation directories.

const fs = require("fs");
const path = require("path");
const sharp = require("sharp");
const GIF = require("sharp-gif2");

const ANIM_DIR = path.resolve(__dirname, "..", "target", "animations");
const OUTPUT_DIR = path.resolve(__dirname, "..", "target", "animations");

async function assembleGif(name) {
  const frameDir = path.join(ANIM_DIR, name);
  const timingPath = path.join(frameDir, "timing.json");

  if (!fs.existsSync(timingPath)) {
    console.error(`No timing.json for ${name} — run render-animations.js first`);
    return;
  }

  const timing = JSON.parse(fs.readFileSync(timingPath, "utf-8"));
  const svgFiles = fs
    .readdirSync(frameDir)
    .filter((f) => f.endsWith(".svg"))
    .sort();

  if (svgFiles.length === 0) {
    console.error(`No SVG frames for ${name}`);
    return;
  }

  // Convert SVGs to sharp instances
  const frames = [];
  for (let i = 0; i < svgFiles.length; i++) {
    const svgPath = path.join(frameDir, svgFiles[i]);
    const svgData = fs.readFileSync(svgPath);
    const sharpFrame = sharp(svgData, { density: 72 });
    frames.push(sharpFrame);
  }

  // Build delay array (ms per frame)
  const delays = svgFiles.map((_, i) => {
    const t = timing[i] ? timing[i].duration_ms : 100;
    return Math.max(t, 20); // GIF minimum ~20ms
  });
  // Add end-of-loop pause
  delays[delays.length - 1] = Math.max(delays[delays.length - 1], 2000);

  // Create animated GIF
  const gifPath = path.join(OUTPUT_DIR, `${name}.gif`);
  try {
    const gif = GIF.createGif({ delay: delays, repeat: 0 });
    gif.addFrame(frames);
    const output = await gif.toSharp();
    await output.toFile(gifPath);

    const stats = fs.statSync(gifPath);
    console.log(
      `${name}: ${svgFiles.length} frames → ${gifPath} (${Math.round(stats.size / 1024)}KB)`
    );
  } catch (err) {
    console.error(`Error creating GIF for ${name}: ${err.message}`);
  }
}

async function main() {
  const args = process.argv.slice(2);

  if (!fs.existsSync(ANIM_DIR)) {
    console.error("No target/animations/ — run the animation tests first.");
    process.exit(1);
  }

  let names;
  if (args.length > 0) {
    names = args;
  } else {
    names = fs
      .readdirSync(ANIM_DIR)
      .filter((f) => {
        const dir = path.join(ANIM_DIR, f);
        return (
          fs.statSync(dir).isDirectory() &&
          fs.existsSync(path.join(dir, "timing.json"))
        );
      });
  }

  if (names.length === 0) {
    console.error("No animations to process. Run render-animations.js first.");
    process.exit(1);
  }

  for (const name of names) {
    await assembleGif(name);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
