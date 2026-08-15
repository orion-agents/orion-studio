---
title: Operator-Managed Admin Controls
description: Admin controls required before offering an Orion Studio organization service.
---

# Operator-Managed Admin Controls

Admin controls are available only when an operator deploys and connects an
organization service. The deployment should fail closed and must document which
controls are enforced on the server rather than only in the desktop UI.

At minimum, an operator should be able to:

- enable or disable hosted model access;
- enable or disable edit prediction and feedback collection;
- enable or disable real-time collaboration;
- prevent prompt, code, and thread sharing;
- set role-based access and spending limits;
- audit policy changes and access decisions.

There is no assumed public Orion dashboard URL. Deployment documentation must
provide the dashboard address, authentication method, and support process.
