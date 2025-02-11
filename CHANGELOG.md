# Change releases

## Version v0.3.0 (2025-02-11)
#### What's New
  - The UI is now fluid and it updates quicker than version 0.2. The known issue in v0.2.1 has been handled.
  - The system is more robust: Introducing handlers for sudden crash/shutdown. Now your saving/loading will be well protected.
  - Introducing adaptive speed handling for low and fast PC devices.
  - *Nerdy stuff*: Unlike all previous versions where every encounter will be logged to a save log, which will wear out your beloved SSD card real fast. This new version uses in-memory save that will only be written to save log every 5 encounters, which saves your card's life span by a huge deal.

#### Known issue
- While crashing will not make files corrupted like GEC, it still does not save your in-memory data to the save log. In other word, sudden crash or forced shut down will make you miss a few encounters (within 5 encounters worth of Pokemon).

## Version v0.2.1 (2025-02-09)
#### Fixes
  - Clean up codes, increase response speed. Although the UI might still feel a bit delayed.
  - Reduce redundancy in using background threads.

#### Known issue
  - Sometimes the UI doesn't synchronize with internal run, which might cause glitch on UI (i.e. buttons do not immediately highlight when hovered, or an encounter is skipped). However, those are only noticeable if you pay attention to them.

## Version v0.2.0 (2025-02-07)
#### What's New
  - GUI is now more responsive during detection, button clicks is in active state all the time, only delay a bit during the count.
  - The fail rate is drastically reduced with the message passing mechanism between background thread and UI.

#### Known issue
  - The internal data updates but won't show up on the UI until you hover your mouse over it.

## Version v0.1.1 (2025-02-06)
#### What's New
  - Screen content detection and capture are now faster and more accurate.
  - Add display on UI for last Pokemon encountered.
  - UI refresh rate is faster.

## Version v0.1.0 (2025-02-05)
#### What's New
  - Create a basic functional GUI based encounter counter.