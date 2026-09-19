---
id: 77
key: v0.6
title: one window on everything
type: milestone
status: backlog
created: 2026-09-15
updated: 2026-09-15
priority: p2
due: 2027-01-11
---

## Ships

One window on everything. A sidebar you can open and close holds every session on the machine, grouped by project, marked live or waiting for you, with counts of what you have not read. It sits alongside the feed rather than on top of it, so you can read one project while watching another.

## Why a panel and not more of what is there

Three surfaces exist and each answers a different question badly:

- **The rail** shows one project's sessions. It cannot tell you another project just went live.
- **The picker** shows everything, but it is modal: it covers the feed, so you cannot look at the list and read at the same time.
- **The merged view** interleaves one project. It is a reading mode, not a way to navigate.

Measured on one machine: 58 projects, three of them active within the same minute. The thing missing is ambient awareness while reading — which is exactly what a persistent panel is for, and exactly what a modal cannot do.

Closed, it costs nothing. That is what makes it worth having: the argument against a sidebar was that it takes a quarter of the width permanently, and a toggle does not.

## Done when

- [ ] A key opens and closes the panel, and the choice survives a restart
- [ ] Every session on the machine is reachable from it, grouped by project
- [ ] It says which sessions are live, and which are waiting for a reply
- [ ] It says how much has arrived since you last looked
- [ ] Focus moves between panel and feed, and it is obvious which has it
- [ ] Searching puts its results in the panel, and choosing one moves the feed
- [ ] The modal picker is gone: one way to browse, not two
- [ ] A narrow terminal still works

## Explicitly not in this milestone

- Reading two feeds side by side. The panel is for navigating, not for a second feed.
- Anything that writes into a session.
- Panels beyond sessions and search. The architecture allows more; this milestone ships two.
