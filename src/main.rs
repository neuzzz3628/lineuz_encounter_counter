extern crate image;
extern crate regex;
extern crate rten_imageio;
extern crate rten_tensor;

mod encounter;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering; // Fix: Import Ordering
use std::thread;

use eframe::egui;
use encounter::{
    encounter_process, get_current_working_dir, load_state, save_state, EncounterState,
    APP_STATE, STATE_IDLE, STATE_ONGOING, STATE_PAUSE, STATE_QUITTING, APP_NAME,
};
use std::{env, error::Error, fs};
use xcap::Window;

fn init_engine() -> Result<ocrs::OcrEngine, Box<dyn Error>> {
    let (detection_path, recognition_path) = get_path_to_models();
    let (detection_model, recognition_model) = load_rten_model(detection_path, recognition_path)?;

    create_engine(detection_model, recognition_model)
}

fn create_engine(
    detection_model: rten::Model,
    recognition_model: rten::Model,
) -> Result<ocrs::OcrEngine, Box<dyn Error>> {
    let engine = ocrs::OcrEngine::new(ocrs::OcrEngineParams {
        detection_model: Some(detection_model),
        recognition_model: Some(recognition_model),
        ..Default::default()
    })?;
    Ok(engine)
}

fn load_rten_model(
    detection_path: String,
    recognition_path: String,
) -> Result<(rten::Model, rten::Model), Box<dyn Error>> {
    let detection_model_data = fs::read(detection_path)?;
    let rec_model_data = fs::read(recognition_path)?;
    let detection_model = rten::Model::load(detection_model_data)?;
    let recognition_model = rten::Model::load(rec_model_data)?;
    Ok((detection_model, recognition_model))
}

fn get_path_to_models() -> (String, String) {
    let (exe_path, path) = get_current_working_dir();

    let detection_path = format!("{}/text-detection.rten", path);
    let detection_path_exe = format!("{}/text-detection.rten", exe_path);
    let recognition_path = format!("{}/text-recognition.rten", path);
    let recognition_path_exe = format!("{}/text-recognition.rten", exe_path);

    match fs::read(&detection_path) {
        Ok(_) => (detection_path, recognition_path),
        _ => (detection_path_exe, recognition_path_exe),
    }
}

fn debug_mode() -> Option<Result<(), Box<dyn Error>>> {
    let (exe_path, path) = get_current_working_dir();
    println!("The current directory is {path} exe path {exe_path}");
    for window in Window::all().unwrap().iter() {
        println!("Window: {:?}", (window.app_name(), window.title()));

        if window.title().to_lowercase() == APP_NAME || window.app_name() == APP_NAME {
            let img = window.capture_image().unwrap();
            let _ = img.save("debug.png");
        }
    }
    Some(Ok(()))
}

pub struct App {
    pub encounter_state: Arc<Mutex<EncounterState>>,
    engine: Arc<ocrs::OcrEngine>,
    worker_thread: Option<thread::JoinHandle<()>>,
}

impl App {
    fn new() -> Self {
        let engine = Arc::new(init_engine().unwrap());
        let encounter_state = Arc::new(Mutex::new(load_state().unwrap_or_default()));
        APP_STATE.store(STATE_IDLE, Ordering::SeqCst); // Initialize the global state

        Self {
            encounter_state,
            engine,
            worker_thread: None,
        }
    }

    fn start_worker(&mut self, ctx: &egui::Context) {
        if self.worker_thread.is_none() {
            let encounter_state = Arc::clone(&self.encounter_state);
            let engine = Arc::clone(&self.engine);
            let ctx = ctx.clone(); // 🔹 Clone `ctx` for use in the worker thread
    
            self.worker_thread = Some(std::thread::spawn(move || {
                while APP_STATE.load(Ordering::SeqCst) == STATE_ONGOING {
                    if let Some(window) = Window::all()
                        .ok()
                        .and_then(|w| w.into_iter().find(|w| encounter::game_exist(w)))
                    {
    
                        if let Ok(mut state) = encounter_state.try_lock() {
                            if let Ok(Some(true)) = encounter_process(&engine, &mut state, &window) {
                                println!("[DEBUG] Encounter detected, requesting repaint!");
                                ctx.request_repaint(); // ✅ Correctly use `ctx` to trigger UI update
                            }
                        } else {
                            println!("[DEBUG] Skipping encounter_process - state locked");
                        }
                    }
                    // println!("[DEBUG] Thread sleeps.");
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                println!("[DEBUG] Worker thread exiting.");
            }));
        }
    }
    
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // 🔹 Title at the top
            ui.heading("Encounter Counter");

            // 🔹 App State Label
            let state_text = match APP_STATE.load(Ordering::SeqCst) {
                STATE_IDLE => "Idle",
                STATE_ONGOING => "Ongoing",
                STATE_PAUSE => "Paused",
                STATE_QUITTING => "Quitting",
                _ => "Unknown",
            };
            ui.label(format!("App State: {}", state_text));

            // 🔹 Buttons in Horizontal Layout
            ui.horizontal(|ui| {
                if ui.button("Start (S)").clicked() {
                    APP_STATE.store(STATE_ONGOING, Ordering::SeqCst);
                
                    // ✅ Ensure the worker thread is properly restarted
                    if self.worker_thread.is_none() {
                        self.start_worker(ctx);
                    }
                }
                
                if ui.button("Pause (P)").clicked() {
                    APP_STATE.store(STATE_PAUSE, Ordering::SeqCst);
                
                    // ✅ Stop worker thread
                    if let Some(handle) = self.worker_thread.take() {
                        let _ = handle.join();
                    }
                
                    // ✅ Reset only `in_encounter` while keeping `mon_stats` and `encounters`
                    if let Ok(mut state) = self.encounter_state.try_lock() {
                        state.in_encounter = false; // Reset only this flag
                        save_state(&state).unwrap_or_default(); // Save updated state
                    }
                }
                
                
                if ui.button("Reset (R)").clicked() {
                    if let Ok(mut state) = self.encounter_state.try_lock() {
                        *state = EncounterState::default();
                        save_state(&state).unwrap_or_default();
                    }
                }
                if ui.button("Quit (Q)").clicked() {
                    APP_STATE.store(STATE_QUITTING, Ordering::SeqCst);
                    if let Some(handle) = self.worker_thread.take() {
                        let _ = handle.join();
                    }
                    std::process::exit(0);
                }
            });

            // 🔹 Ensure "Total Encounters" is always up-to-date
            let total_encounters = {
                if let Ok(state) = self.encounter_state.try_lock() {
                    state.encounters // Use the latest value
                } else {
                    self.encounter_state.lock().unwrap().encounters // Clone safely even if locked
                }
            };
            ui.label(format!("Total Encounters: {}", total_encounters));

            // 🔹 Separator for better UI clarity
            ui.separator();

            // 🔹 Top 8 Encounters Section
            ui.heading("Top 8 Encounters");
            if let Ok(state) = self.encounter_state.try_lock() {
                let mut top_encounters: Vec<(&String, &u32)> = state.mon_stats.iter().collect();
                top_encounters.sort_by(|a, b| b.1.cmp(a.1));

                for (i, (mon, count)) in top_encounters.iter().take(8).enumerate() {
                    ui.label(format!("{}. {} - {}", i + 1, mon, count));
                }
            }
        });

        // 🔹 Ensure `start_worker()` restarts if it's stopped but should be running
        if APP_STATE.load(Ordering::SeqCst) == STATE_ONGOING && self.worker_thread.is_none() {
            self.start_worker(ctx);
        }
    }
}


fn main() -> Result<(), Box<dyn Error>> {
    let is_debug = env::args().find(|arg| arg == "debug");

    if is_debug.is_some() {
        if let Some(value) = debug_mode() {
            return value;
        }
    }

    if let Some(_window) = Window::all()
        .ok()
        .and_then(|w| w.into_iter().find(|w| encounter::game_exist(&w)))
    {
        let app = App::new();
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([300.0, 350.0]), // Width x Height
            ..Default::default()
        };
        
        eframe::run_native(
            "Encounter Counter",
            native_options,
            Box::new(|_cc| Ok(Box::new(app))),
        )?;
        Ok(())
    } else {
        eprintln!("{} game not found", APP_NAME);
        std::process::exit(1);
    }
}