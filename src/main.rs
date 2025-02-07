extern crate image;
extern crate regex;
extern crate rten_imageio;
extern crate rten_tensor;

mod encounter;
use std::sync::atomic::Ordering; // Fix: Import Ordering
use std::sync::{Arc, Mutex};
use std::thread;
use std::sync::mpsc;
use eframe::egui;
use encounter::{
    encounter_process, get_current_working_dir, load_state, save_state, EncounterState, APP_NAME,
    APP_STATE, STATE_IDLE, STATE_ONGOING, STATE_PAUSE, STATE_QUITTING,
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
    worker_rx: Option<std::sync::mpsc::Receiver<EncounterState>>, // ✅ Add the receiver
    last_rendered_state: EncounterState, // ✅ Cache the last known state
}


impl App {
    fn new() -> Self {
        let engine = Arc::new(init_engine().unwrap());
        let encounter_state = Arc::new(Mutex::new(load_state().unwrap_or_default()));
        let last_rendered_state = encounter_state.lock().unwrap().clone(); // ✅ Proper initialization
    
        APP_STATE.store(STATE_IDLE, Ordering::SeqCst);
    
        Self {
            encounter_state,
            engine,
            worker_thread: None,
            worker_rx: None,
            last_rendered_state, // ✅ Use self.last_rendered_state correctly
        }
    }
    

    fn start_worker(&mut self) {
        if self.worker_thread.is_none() {
            let encounter_state = Arc::clone(&self.encounter_state);
            let engine = Arc::clone(&self.engine);
            let (tx, rx) = mpsc::channel(); // 🔹 Create message channel

            self.worker_rx = Some(rx);
    
            self.worker_thread = Some(std::thread::spawn(move || {
                while APP_STATE.load(Ordering::SeqCst) == STATE_ONGOING {
                    if let Some(window) = Window::all()
                        .ok()
                        .and_then(|w| w.into_iter().find(|w| encounter::game_exist(w)))
                    {
                        if let Ok(mut state) = encounter_state.lock() {
                            if let Ok(Some(true)) = encounter_process(&engine, &mut state, &window, &tx) {
                                let _ = tx.send(state.clone());
                            }
                        }
                    }
                    // std::thread::sleep(std::time::Duration::from_millis(10)); // 🔹 Slightly increase delay to reduce CPU usage
                }
                println!("[DEBUG] Worker thread exiting.");
            }));
        }
    }
    
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ✅ Non-blocking check for new state updates
        if let Some(rx) = &self.worker_rx {
            if let Ok(new_state) = rx.recv_timeout(std::time::Duration::from_millis(5)){
                if let Ok(mut state) = self.encounter_state.lock() {
                    if *state != new_state {  // ✅ Only update if state has changed
                        *state = new_state;
                        self.last_rendered_state = state.clone(); // ✅ Store cached state
                        ctx.request_repaint(); // ✅ Repaint only when state changes
                    }
                }
            }
        }
    
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Encounter Counter");
    
            let state_text = match APP_STATE.load(Ordering::SeqCst) {
                STATE_IDLE => "Idle",
                STATE_ONGOING => "Ongoing",
                STATE_PAUSE => "Paused",
                STATE_QUITTING => "Quitting",
                _ => "Unknown",
            };
            ui.label(format!("App State: {}", state_text));
    
            ui.horizontal(|ui| {
                if ui.button("Start (S)").clicked() {
                    APP_STATE.store(STATE_ONGOING, Ordering::SeqCst);
                    if self.worker_thread.is_none() {
                        self.start_worker();
                    }
                }
    
                if ui.button("Pause (P)").clicked() {
                    APP_STATE.store(STATE_PAUSE, Ordering::SeqCst);
                    if let Some(handle) = self.worker_thread.take() {
                        let _ = handle.join();
                    }
                    self.worker_rx = None; // ✅ Reset receiver when pausing
                }
    
                if ui.button("Reset (R)").clicked() {
                    if let Ok(mut state) = self.encounter_state.lock() {
                        *state = EncounterState::default();
                        save_state(&state).unwrap_or_default();
                        
                        self.last_rendered_state = state.clone(); // ✅ Ensure UI updates
                        ctx.request_repaint(); // ✅ Force UI refresh
                    }
                }
    
                if ui.button("Quit (Q)").clicked() {
                    APP_STATE.store(STATE_QUITTING, Ordering::SeqCst);
                    if let Some(handle) = self.worker_thread.take() {
                        let _ = handle.join();
                    }
                    self.worker_rx = None; // ✅ Reset receiver
                    std::process::exit(0);
                }
            });
    
            // ✅ Read from cached state instead of locking mutex each frame
            let state = &self.last_rendered_state;
            ui.separator();
            ui.label(format!("Total Encounters: {}", state.encounters));
            ui.label(format!("Last Encounters: {}", state.last_encounter.join(", ")));
            ui.separator();
    
            ui.heading("Top 8 Encounters");
            let mut top_encounters: Vec<(&String, &u32)> = state.mon_stats.iter().collect();
            top_encounters.sort_by(|a, b| b.1.cmp(a.1));
    
            for (i, (mon, count)) in top_encounters.iter().take(8).enumerate() {
                ui.label(format!("{}. {} - {}", i + 1, mon, count));
            }
        });
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
