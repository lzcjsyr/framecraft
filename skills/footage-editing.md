# Footage editing

Prefer supplied video and image assets that support the requested story. Use real source IDs and keep video in/out points within the probed source duration. A still image starts at zero and can be held for the scene duration. When a needed visual is missing, provide a specific image prompt so the configured image API can create it. Do not treat the reference video as output footage unless it was also imported as an asset.

Use supplied audio as a continuous soundtrack when appropriate. Otherwise, when narration is enabled, write short original narration for MiMo to speak per scene. Leave narration empty when disabled. ASR transcripts provide approximate segment timing, not word timings. Do not invent observed speech or claim precise synchronization from segment transcripts.

The renderer supports straight cuts, held images, aspect fitting, original audio or a separate soundtrack/narration, and captions as a sidecar SRT. It does not support effects, animated overlays, transitions, or generated music. Follow the current task's storyboard schema and explain unsupported requests in warnings.

On macOS, use the native VideoToolbox rendering path. Audio processing, padding, caption files, and assembly can still use CPU work; do not claim the entire pipeline is GPU-only.
