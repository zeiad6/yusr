// Names match `Permission` in crates/yusr-server/src/auth.rs. The server sends each user's
// effective set at login and on /auth/me; it is used only to hide UI the server would refuse
// anyway. The server stays the authority.

export const PERMISSION_GROUPS = {
  reports: ["reports.read"],
  ledger: ["ledger.read", "ledger.post", "ledger.reverse", "accounts.manage"],
  funds: ["funds.manage", "budgets.manage", "fx.manage"],
  donors: ["donors.read", "donors.manage"],
  donations: ["donations.read", "donations.record", "donations.edit", "payments.checkout"],
  sponsorships: ["sponsorships.read", "sponsorships.manage", "sponsorships.pay"],
  cases: ["cases.read", "cases.manage", "aid.distribute", "vouchers.redeem"],
  volunteers: ["volunteers.read", "volunteers.manage", "volunteers.hours"],
  zakat: ["zakat.calculate"],
  invoices: ["invoices.read", "invoices.issue"],
  archive: ["archive.read", "archive.write", "archive.void", "archive.confidential"],
  letters: ["letters.read", "letters.write", "letters.issue"],
  audit: ["audit.read"],
  admin: ["users.manage", "settings.manage", "backup.create", "backup.restore"],
  advisor: ["advisor.use", "advisor.manage"],
} as const;

export type PermissionGroup = keyof typeof PERMISSION_GROUPS;
export type Permission = (typeof PERMISSION_GROUPS)[PermissionGroup][number];

export const ALL_PERMISSIONS = Object.values(PERMISSION_GROUPS).flat() as Permission[];

const ROLE_DEFAULTS: Record<string, Set<string>> = {
  admin: new Set(ALL_PERMISSIONS),
  accountant: new Set(
    ALL_PERMISSIONS.filter(
      (p) =>
        ![
          "users.manage",
          "cases.manage",
          "volunteers.manage",
          "backup.restore",
          "advisor.manage",
        ].includes(p)
    )
  ),
  case_worker: new Set([
    "reports.read",
    "cases.read",
    "cases.manage",
    "aid.distribute",
    "vouchers.redeem",
    "donors.read",
    "donations.read",
    "sponsorships.read",
    "volunteers.read",
    "volunteers.manage",
    "volunteers.hours",
    "zakat.calculate",
    "archive.read",
    "archive.write",
    "letters.read",
    "letters.write",
    "advisor.use",
  ]),
  auditor: new Set([
    "reports.read",
    "ledger.read",
    "donors.read",
    "donations.read",
    "sponsorships.read",
    "cases.read",
    "volunteers.read",
    "zakat.calculate",
    "invoices.read",
    "archive.read",
    "archive.confidential",
    "letters.read",
    "audit.read",
    "advisor.use",
  ]),
  viewer: new Set(["reports.read", "zakat.calculate", "advisor.use"]),
};

export function allows(role: string | undefined, permission: Permission | string): boolean {
  if (!role) return false;
  const set = ROLE_DEFAULTS[role];
  return set ? set.has(permission) : false;
}
