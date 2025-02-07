use core::panic;
use image::{DynamicImage, RgbImage};
use ocrs::{ImageSource, OcrEngine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::sync::atomic::AtomicU8;
use std::sync::mpsc;
use rayon::prelude::*;
use xcap::Window; // Required for io::Error

pub const APP_NAME: &str = "pokemmo";
pub const JAVA: &str = "java";

// AtomicU8 for global app state
pub static APP_STATE: AtomicU8 = AtomicU8::new(STATE_IDLE);

// Constants for AtomicU8 state
pub const STATE_IDLE: u8 = 0;
pub const STATE_ONGOING: u8 = 1;
pub const STATE_PAUSE: u8 = 2;
pub const STATE_QUITTING: u8 = 3;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct EncounterState {
    pub encounters: u32,
    pub last_encounter: Vec<String>,
    pub mon_stats: HashMap<String, u32>,
    pub debug: bool,
    pub in_encounter: bool,
}

impl Default for EncounterState {
    fn default() -> Self {
        Self {
            encounters: 0,
            last_encounter: vec![],
            mon_stats: HashMap::new(),
            debug: false,
            in_encounter: false,
        }
    }
}

pub fn game_exist(w: &Window) -> bool {
    let name = w.app_name().to_lowercase();
    let title = w.title().to_lowercase();
    [APP_NAME, JAVA].contains(&name.as_str()) || [APP_NAME, JAVA].contains(&title.as_str())
}

pub fn get_current_working_dir() -> (String, String) {
    match (std::env::current_exe(), std::env::current_dir()) {
        (Ok(exe_path), Ok(path)) => (
            exe_path.parent().unwrap().display().to_string(),
            path.display().to_string(),
        ),
        _ => panic!("can't find current directory"),
    }
}

pub fn load_state() -> Result<EncounterState, Box<dyn Error>> {
    let state_json = fs::read_to_string("state.json")?;
    let state = serde_json::from_str(&state_json)?;
    Ok(state)
}

pub fn save_state(state: &EncounterState) -> Result<(), Box<dyn Error>> {
    let state_json = serde_json::to_string(state)?;
    fs::write("state.json", state_json)?;
    Ok(())
}

fn capture_bottom(debug: bool, window: &Window) -> Result<RgbImage, Box<dyn Error>> {
    let screen_height = window.height();
    let screen_width = window.width();

    // Define start and end points for cropping
    let start_x = (screen_width as f32 * 0.06) as u32;
    let end_x = (screen_width as f32 * 0.7) as u32;
    let start_y = (screen_height as f32 * 0.6) as u32; // Start from 50% (middle)
    let end_y = (screen_height as f32 * 0.78) as u32; // Up to 80% of screen height

    let img = window.capture_image()?;
    let img = DynamicImage::ImageRgba8(img)
        .crop(start_x, start_y, end_x - start_x, end_y - start_y) // Capture middle-bottom portion
        .grayscale()
        .to_rgb8();

    if debug {
        img.save("debug_bottom.png")?;
    }

    Ok(img)
}


fn capture_screen(debug: bool, window: &Window) -> Result<RgbImage, Box<dyn Error>> {
    let screen_height = window.height();
    let screen_width = window.width();

    // Define start and end points for cropping
    let start_x = (screen_width as f32 * 0.06) as u32;
    let end_x = (screen_width as f32 * 0.94) as u32;
    let start_y = (screen_height as f32 * 0.06) as u32;
    let end_y = (screen_height as f32 * 0.3) as u32; 

    let img = window.capture_image()?;
    let img = DynamicImage::ImageRgba8(img)
        .crop(start_x, start_y, end_x - start_x, end_y - start_y) // Capture top portion
        .grayscale()
        .to_rgb8();

    if debug {
        img.save("debug.png")?;
    }

    Ok(img)
}

pub fn get_wild(engine: &OcrEngine, data: RgbImage) -> Result<bool, Box<dyn Error>> {
    let small_img = DynamicImage::ImageRgb8(data).to_rgb8();
    let img = ImageSource::from_bytes(small_img.as_raw(), small_img.dimensions())?;
    let ocr_input = engine.prepare_input(img)?;
    let word_rects = engine.detect_words(&ocr_input)?;
    let line_rects = engine.find_text_lines(&ocr_input, &word_rects);
    let line_texts = engine.recognize_text(&ocr_input, &line_rects)?;

    // 🔹 Parallel iteration for faster processing
    let contains_wild = line_texts
        .par_iter() // Parallel iteration for speed
        .flatten()
        .map(|line| line.to_string().to_lowercase()) // Convert to lowercase
        .any(|line| line.contains("a wild")); // Check for "a wild"
    Ok(contains_wild)
}

fn get_mons(engine: &OcrEngine, data: RgbImage) -> Result<Vec<String>, Box<dyn Error>> {

    let small_img = DynamicImage::ImageRgb8(data).to_rgb8();
    let img = ImageSource::from_bytes(small_img.as_raw(), small_img.dimensions())?;
    let ocr_input = engine.prepare_input(img)?;
    let word_rects = engine.detect_words(&ocr_input)?;
    let line_rects = engine.find_text_lines(&ocr_input, &word_rects);
    let line_texts = engine.recognize_text(&ocr_input, &line_rects)?;

    // 🔹 Use parallel iterator to process text lines faster
    let mons: Vec<String> = line_texts
        .par_iter() // Parallel iteration
        .flatten()
        .map(|l| l.to_string().to_lowercase())
        .filter(|line| line.contains("lv.") || line.contains("nv.") || line.contains("niv."))
        .flat_map(|line| {
            line.split_whitespace()
                .collect::<Vec<_>>()
                .windows(2)
                .filter_map(|w| {
                    if (w[1] == "lv." || w[1] == "nv." || w[1] == "niv.") && w[0].len() > 1 {
                        Some(w[0].to_string())
                    } else {
                        None
                    }
                })
                .collect::<Vec<String>>() // Collect each filtered window into Vec<String>
        })
        .collect(); // Collect all results into `mons`
    Ok(mons)
}

pub fn encounter_process(
    engine: &OcrEngine,
    state: &mut EncounterState,
    window: &Window,
    tx: &mpsc::Sender<EncounterState>, // 🔹 Add a Sender to notify the UI
) -> Result<Option<bool>, Box<dyn Error>> {

    if !state.in_encounter {
        let cropped_wild = capture_bottom(state.debug, window)?;
        let wilds = get_wild(engine, cropped_wild)?;
        if wilds {
            state.in_encounter = true; // **Mark the start of an encounter**
            std::thread::sleep(std::time::Duration::from_millis(10)); // Small delay for stability
        }
    }

    if state.in_encounter {
        let cropped_image = capture_screen(state.debug, window)?;
        let mons = get_mons(engine, cropped_image)?;

        if !mons.is_empty() && state.last_encounter.is_empty() {
                state.encounters += mons.len() as u32;
                state.last_encounter = mons.clone();
                for mon in mons {
                    *state.mon_stats.entry(mon.clone()).or_insert(0) += 1;
                }
                save_state(state)?;
                tx.send(state.clone()).ok(); // ✅ Send updated state to the GUI
                return Ok(Some(true));
        } else {
            if !state.last_encounter.is_empty() {
                state.in_encounter = false;
                state.last_encounter.clear();
            }
        }
    }
    Ok(Some(false))
}
