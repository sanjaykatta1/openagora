#!/usr/bin/env python3
"""Write portable SHA-256 sidecars for explicitly named release artifacts."""
import hashlib
import pathlib
import sys

for arg in sys.argv[1:]:
    path = pathlib.Path(arg)
    digest = hashlib.file_digest(path.open('rb'), 'sha256').hexdigest()
    path.with_name(path.name + '.sha256').write_text(f'{digest}  {path.name}\n', encoding='ascii')
