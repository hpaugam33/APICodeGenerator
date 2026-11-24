"""Command line interface.

Usage:
    python -m apigen --openapi specs/order-service.openapi.yaml \
                     --asyncapi specs/order-events.asyncapi.yaml \
                     --target both --out generated \
                     --package com.example.orderservice --crate order_service

Generation is a FULL REGENERATE: each target directory is wiped and rebuilt,
so the specs remain the single source of truth. Keep hand-written code in
files/packages the generator does not own.
"""

import argparse
import os
import shutil
import sys

from . import __version__
from .loader import load_all
from .naming import kebab, snake
from .emit import EMITTERS


def build_parser():
    p = argparse.ArgumentParser(
        prog="apigen",
        description="Generate service API interfaces + data models from OpenAPI/AsyncAPI specs "
                    "(as exported from Sparx EA data models and information flows).")
    p.add_argument("--openapi", action="append", default=[], metavar="FILE",
                   help="OpenAPI 3.x spec file (repeatable)")
    p.add_argument("--asyncapi", action="append", default=[], metavar="FILE",
                   help="AsyncAPI 2.x/3.x spec file (repeatable)")
    p.add_argument("--target", choices=sorted(EMITTERS) + ["both"], default="both",
                   help="which language to generate (default: both)")
    p.add_argument("--out", default="generated", help="output root directory (default: generated)")
    p.add_argument("--package", default=None, help="Java base package (Quarkus)")
    p.add_argument("--group-id", default="com.example", help="Maven groupId (Quarkus)")
    p.add_argument("--artifact", default=None, help="Maven artifactId (Quarkus)")
    p.add_argument("--crate", default=None, help="Rust crate name")
    p.add_argument("--api-name", default=None, help="override API name (default: info.title)")
    p.add_argument("--version", action="version", version="apigen " + __version__)
    return p


def main(argv=None):
    args = build_parser().parse_args(argv)
    if not args.openapi and not args.asyncapi:
        print("error: pass at least one --openapi or --asyncapi file", file=sys.stderr)
        return 2

    ir = load_all(args.openapi, args.asyncapi, api_name=args.api_name)
    command = "python -m apigen " + " ".join(sys.argv[1:]) if argv is None else "python -m apigen ..."

    targets = sorted(EMITTERS) if args.target == "both" else [args.target]
    print(f"Loaded {ir.api_name} v{ir.version}: "
          f"{len(ir.models)} models, {len(ir.endpoints)} REST endpoints, {len(ir.flows)} message flows")
    for t in targets:
        out_dir = os.path.join(args.out, t)
        if os.path.isdir(out_dir):
            shutil.rmtree(out_dir)  # full regenerate: spec is the source of truth
        os.makedirs(out_dir, exist_ok=True)
        kwargs = {"command": command}
        if t == "quarkus":
            kwargs.update(package=args.package or "com.example." + snake(ir.api_name).replace("_", ""),
                          group_id=args.group_id,
                          artifact=args.artifact or kebab(ir.api_name))
        else:
            kwargs.update(crate_name=args.crate or snake(ir.api_name))
        written = EMITTERS[t](ir, out_dir, **kwargs)
        print(f"  [{t}] wrote {len(written)} files -> {out_dir}/")
        for rel in sorted(os.path.relpath(p, out_dir) for p in written):
            print(f"      {rel}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
