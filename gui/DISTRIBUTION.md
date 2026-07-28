# Impulse Response Capture GUI

This folder is a self-contained Windows build. Keep the executable, DLLs, PRI files, and
language-resource directories together. Run `rust-impulse-response-gui.exe` from this folder.

For a stereo capture:

1. Keep the original software filter and its routing enabled, and silence other applications.
2. Under **Send impulses to**, select the virtual playback endpoint that applications normally
   use.
3. Under **Capture processed output from**, select the final physical playback endpoint.
4. Confirm that both endpoints show two channels and the same sample rate.
5. Capture the 2×2 matrix. A successful capture writes two WAV files and
   `equalizer-apo.txt` to the selected output directory.
6. Copy that entire output directory under Equalizer APO's `config` directory.
7. Disable the original filter and include the generated configuration, for example:
   `Include: captured-filter\equalizer-apo.txt`.
8. Check the configuration in Equalizer APO's editor, then test at low volume.

The endpoint sample rate must match the sample rate used during capture.
