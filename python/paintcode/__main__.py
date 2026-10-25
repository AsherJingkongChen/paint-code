"""python -m paintcode SCENE -o OUT [-o OUT ...]; see `python -m paintcode --help`."""

import sys

from ._core import main

raise SystemExit(main(sys.argv[1:]))
