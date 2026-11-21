import argparse
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from scripts import integration_plan, rust_adapter
from scripts.validate_candidate import identity
from scripts.verify_acceptance import digest, read_json, validate_contract


def verify(base):
    if Path.cwd().resolve() != ROOT or rust_adapter.ROOT != ROOT:
        raise ValueError("Integration verification must run in its exact candidate checkout")
    source = identity()
    expected_plan = integration_plan.plan(base)
