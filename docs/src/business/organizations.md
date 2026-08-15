---
title: Organizations
description: Deployment requirements for Orion Studio organizations, membership, and billing roles.
---

# Organizations

Organizations require an operator-deployed account and authorization service.
No public Orion organization dashboard is assumed to exist.

A deployment that enables organizations must define:

- how organizations are created and deleted;
- who can invite, remove, and suspend members;
- how owners, admins, billing managers, and members are authorized;
- how audit logs, billing, and data retention work;
- what happens when the service is unavailable.

The desktop application must remain usable for local workflows when an
organization service is not configured. See [Roles and Permissions](../roles.md)
and [Admin Controls](./admin-controls.md).
