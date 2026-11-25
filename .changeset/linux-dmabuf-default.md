---
"stroke": patch
---

Fix Stroke Nightly exiting at startup on Wayland with NVIDIA ("Error 71 (Protocol error) dispatching to Wayland display"). Linux turns off WebKitGTK's DMA-buf renderer again by default, set at the very start of the process. Set `WEBKIT_DISABLE_DMABUF_RENDERER=0` to use the GPU renderer.
