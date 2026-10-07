#!/usr/bin/env python3
import runpy
from pathlib import Path
import sys
sys.argv = [sys.argv[0], str(Path(__file__).resolve().parent)] + sys.argv[1:]
runpy.run_path(str(Path.home()/".cursor/skills/code-tour-builder/scripts/build_code_tour.py"), run_name="__main__")
