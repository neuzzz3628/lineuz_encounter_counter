# Neucounter counter 0.0.1

## Description
Encounter counter for the Pokemmo, Linux version.
It is a simple tool to keep track of the number of encounters in the game.

## Features
- Automaticaly count the number of encounters after clicking Start.
- Can be paused in case you don't want to count irrelevant to them (when you do something else in game mid-shunt)
- The current shunt is auto saved and will be loaded upon new activation of the app.
- Can reset


## How to use
Currently worked for Linux only, haven't tested for other systems.
- When app is first opened, you need to click Start for it to start counting.
- During the count, you can press Pause to pause the counter
- It's best if you click Pause before Resetting or Quitting, although I haven't seen any issue with clicking them straight.

### Linux
- open terminal
- go to the directory of app
- run the following command
```bash
cargo run --release
```

> [!NOTE]
Highly recommended to play PokeMMO that occupies at least 60% of your PC/Laptop screen's width and full height.

## How build it from source 

### Linux
1. Install dependencies
Ubuntu / Mint / Debian / PopOS
```bash
sudo apt-get install build-essential libxcb-shm0-dev libxcb-randr0-dev xcb git libxcb1 libxrandr2 libdbus-1-3
```

### All platforms
1. Clone the repository
2. Install Rust language from [here](https://www.rust-lang.org/tools/install) 
3. Run the following command in the terminal
```bash
git clone github.com/neuzzz3628/rencounter_counter-adjusted-
cd rencounter_counter-adjusted
cargo run --release
```

## TODO
- [x] Replace TUI with GUI
- [x] GUI operates normally
- [ ] Fix issues where it misread name and accept them to the counter
- [ ] Fix button clicks that are a bit slow in responsiveness
- [ ] Grid-based
- [ ] Add sprites
- [ ] Allow theme customization

### KNOWN ISSUES
- Sometimes a wild encounter is not recognized, hence no count. Tend to happen if you alt-tab mid-encounter animation. Fail rate < 10%
- The button clicks are a bit lagging behind due to 

### THINGS THAT ARE NOT ISSUES
- GUI still works even if minimized, however the GUI only updates when you un-minimize it.
- When you click to the chat box to ... chat, the battle screen will move to the back by 1 layer. It's no issue if you play full screen but if you play in half-screen mode, it's likely that all the texts at the top left corner as well as active shiny charm logos (or any active logos) will be on top of battle screen. Meaning the Pokemon's names will be covered by these texts -> either the app can't read or it recognize gibberish names.
