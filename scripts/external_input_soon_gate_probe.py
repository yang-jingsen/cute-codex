"""Reuse actual interruption/queue/restart oracle with generic Soon delivery."""

import functools
import runpy
from pathlib import Path
import external_input_probe_support as h

h.envelope = functools.partial(h.envelope, delivery="soon")
runpy.run_path(
    str(Path(__file__).with_name("external_input_gate_probe.py")), run_name="__main__"
)
