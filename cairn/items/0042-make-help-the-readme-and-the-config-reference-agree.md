---
id: 42
title: Make --help, the README and the config reference agree
type: docs
status: done
milestone: v1.0
assignee: Oddur Sigurdsson
created: 2026-09-14
updated: 2026-09-15
priority: p1
effort: m
area: cli
---

## Problem

Three descriptions of the same tool drift apart, and the generated config file is a fourth.

## Proposal

Reconcile them against the code, and add a test that fails when a documented flag or config key no longer exists.

## Acceptance criteria

- [x] Every subcommand and flag in the README exists
- [x] Every config key is documented with its default
- [x] A test catches the next drift

## 2026-09-15

Two tests keep them honest: one asserts every command the README shows in a fenced block exists and every command is mentioned somewhere, the other asserts every key of the written config appears in the README. Both found real drift — the README still said 1b6697c6    ~/Code/astralia                                 just now     18M  claude
41208a28    ~/Code/moan                                     just now      5M  claude
cc6bbec4    ~/Code/unifont                                  just now     34M  claude
2c2e6a91    ~/Code/twerp                                    just now     54M  claude
e42eb817    ~/Code/harrow                                   just now      5M  claude
477eb2c9    ~/Code/perfect                                    2m ago     40M  claude
2c8fe090    ~/Code/quarry                                     6m ago     20M  claude
b45a052e    ~/Code/x-post                                     7m ago    468K  codex
00ec101b    ~/Code/NetherC                                    9m ago     33M  codex
de5cded7    ~/Code/multi-pic                                 20m ago     34M  claude
8a549fb6    ~/Code/NetherC                                   20m ago     15M  claude
c1191035    ~/Code/turborust                                 20m ago     19M  claude
fc011cef    ~/Code                                           20m ago      3M  claude
7d95e327    ~/Code/trafford                                  20m ago     15M  claude
4e100c5b    ~/Code/brainiac                                  20m ago      2M  claude
8d11dca5    ~/Code/kernelis                                  20m ago     27M  claude
26edba5e    ~/Code/triblenka                                 20m ago      3M  claude
c84d4a4b    ~/Code/yoghurt                                   20m ago      6M  claude
ccd11479    ~/Code/harrow                                    20m ago      2M  claude
22bf6d6d    ~/Code/polkadot                                   1d ago      4M  codex
bdb781c4    ~/Code/ruler                                      1d ago    451K  codex
7316c380    ~/Code/x-post                                     1d ago     60K  codex
c6e6f0e3    ~/Code/ascii-screensaver                          1d ago     40M  claude
a645125d    ~/Code/nun                                        1d ago      2M  claude
6d6fd089    ~/Code/scrubb                                     1d ago      1M  claude
b2fee418    ~/Code/backpage                                   4d ago      8M  claude
9b7a5837    ~/Code/clackson                                   4d ago    521K  claude
a6b5ad8b    ~/Code/nolan                                      4d ago    779K  claude
a3d7a44b    ~/Code/xui                                        4d ago      1M  claude
1b543752    ~/Code/honko                                      4d ago    803K  claude, and nine settings were undocumented.
