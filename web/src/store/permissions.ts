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
