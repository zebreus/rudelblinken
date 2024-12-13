#!/usr/bin/env bash
wit-bindgen rust --format --out-dir src --disable-custom-section-link-helpers --world rudel rudel.wit --pub-export-macro
