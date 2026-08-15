---
title: Organization Roles and Permissions
description: Suggested authorization roles for an operator-deployed Orion Studio organization service.
---

# Organization Roles and Permissions

Roles apply only after an operator deploys an Orion Studio organization
service. The desktop application does not create a public organization account
by itself.

An operator may define roles such as owner, admin, billing manager, and member.
Each permission must be enforced server-side, documented, auditable, and tested
against privilege escalation. The deployment must also define transfer of
ownership, account recovery, member removal, and service-unavailable behavior.

See [Organizations](./business/organizations.md) and
[Admin Controls](./business/admin-controls.md).
