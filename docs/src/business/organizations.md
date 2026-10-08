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

Your personal organization always stays active. Joining a Orion Studio Business organization doesn't replace or affect it.

In the Orion Studio editor, an organization menu in the title bar shows your current organization by name. Click it to see all your organizations and switch between them.

## Multiple Organizations

A Orion Studio account can belong to more than one organization at the same time. If you're invited to a second organization while already a member of one, you join both. Each organization has its own subscription, billing, and admin controls.

To switch organizations in the dashboard, use the org switcher in the top-left corner. In the Orion Studio editor, click the organization name in the title bar to see all your organizations and move between them.

## Creating an organization

To create an organization, go to [dashboard.zed.dev/create-organization](https://dashboard.zed.dev/create-organization). The person who creates the organization becomes its owner.

If you don't have a payment method on file, you'll be taken through a checkout flow. If one is already on file, that step is skipped. After that, you'll land on an invite page to add your first members.

## Inviting members

Members are invited by email address. When an invite is accepted, the member's Orion Studio account joins the organization. Owners, admins, and members count toward paid Business seats. Billing Managers can access billing without a paid Business seat.

To invite a member:

1. Go to the Members page in your organization dashboard.
2. Select **+ Invite Member**.
3. Enter the member's email address and choose a role.
4. They'll receive an email with a link to join.

After accepting, they authenticate with their GitHub account and are added to the organization. For details on what each role can do, see [Roles & Permissions](../roles.md).

## Managing members

Owners and admins can manage members from the Members page in the dashboard.

### Changing a member's role

1. On the Members page, find the member.
2. Open the menu and select a new role.

### Removing a member

1. On the Members page, find the member.
2. Select **Remove** and confirm.

Removing a member ends their access to the organization's subscription, billing, and admin-managed settings for that organization. Their personal Orion Studio account and any other organization memberships are unaffected.

## Organization Dashboard

The dashboard shows your members, roles, and billing. Owners and admins have full access. Billing Managers have [billing](../account/billing.md) access. Members have no dashboard access.
