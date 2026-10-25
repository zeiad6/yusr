import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { KeyRound, Plus } from "lucide-react";
import { api } from "../api/client";
import { ROLES, type Role, type User } from "../api/types";
import Modal from "../components/Modal";
import { toastSuccess } from "../components/Toast";
import { Badge, DataTable, ErrorNote, Field, PageHeader, useLoad, useSubmit } from "../components/ui";
import { FormActions } from "../components/forms";
import { useAuthStore } from "../store/auth";
import { PERMISSION_GROUPS, type PermissionGroup } from "../store/permissions";

/** crates/yusr-server/src/auth.rs MIN_PASSWORD_LEN */
export const MIN_PASSWORD_LEN = 10;

export default function Users() {
  const { t } = useTranslation();
  const me = useAuthStore((s) => s.session?.username);
  const [editing, setEditing] = useState<User | "new" | null>(null);
  const [granting, setGranting] = useState<User | null>(null);
  const { data, error, reload } = useLoad(() => api.get<User[]>("/users"));

  const locked = (u: User) => !!u.locked_until && new Date(u.locked_until) > new Date();

  return (
    <div className="page">
      <PageHeader title={t("users.title")}>
        <button type="button" className="btn btn-primary" data-shortcut="new" onClick={() => setEditing("new")}>
          <Plus size={16} /> {t("users.new")}
        </button>
      </PageHeader>
      {error && <ErrorNote message={error} onRetry={reload} />}
      <DataTable
        rows={data ?? []}
        rowKey={(u) => u.id}
        onRowClick={setEditing}
        columns={[
          { header: t("auth.username"), cell: (u) => (u.username === me ? `${u.username} (${t("users.you")})` : u.username) },
          { header: t("common.name"), cell: (u) => u.full_name },
          { header: t("users.role"), cell: (u) => t(`roles.${u.role}`) },
          {
            header: t("common.status"),
            text: (u) => (u.is_active ? t("common.active") : t("common.inactive")),
            cell: (u) =>
              locked(u) ? (
                <Badge tone="warning">{t("users.locked")}</Badge>
              ) : (
                <Badge tone={u.is_active ? "success" : "secondary"}>{u.is_active ? t("common.active") : t("common.inactive")}</Badge>
              ),
          },
          {
            header: t("users.permissions"),
            text: (u) => (u.permissions ? t("users.customPermissions", { count: u.permissions.length }) : t("users.roleDefaults")),
            cell: (u) => (
              <span className="action-btns">
                <Badge tone={u.permissions ? "info" : "secondary"}>
                  {u.role === "admin" ? t("users.allPermissions") : u.permissions ? t("users.customPermissions", { count: u.permissions.length }) : t("users.roleDefaults")}
                </Badge>
                {u.role !== "admin" && (
                  <button
                    type="button"
                    className="btn btn-sm btn-ghost"
                    onClick={(e) => {
                      e.stopPropagation();
                      setGranting(u);
                    }}
                  >
                    <KeyRound size={14} /> {t("users.editPermissions")}
                  </button>
                )}
              </span>
            ),
          },
          { header: t("users.createdAt"), cell: (u) => u.created_at.slice(0, 10) },
        ]}
      />
      {granting && <PermissionsEditor user={granting} onClose={() => setGranting(null)} onSaved={reload} />}
      {editing && <UserForm user={editing === "new" ? null : editing} onClose={() => setEditing(null)} onSaved={reload} />}
    </div>
  );
}

function UserForm({ user, onClose, onSaved }: { user: User | null; onClose: () => void; onSaved: () => void }) {
  const { t } = useTranslation();
  const [form, setForm] = useState({ username: user?.username ?? "", full_name: user?.full_name ?? "", role: (user?.role ?? "viewer") as Role, is_active: user?.is_active ?? true, password: "" });
  const { busy, run } = useSubmit();
  const passwordOk = user ? form.password === "" || form.password.length >= MIN_PASSWORD_LEN : form.password.length >= MIN_PASSWORD_LEN;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      if (user) {
        await api.patch(`/users/${user.id}`, {
          full_name: form.full_name !== user.full_name ? form.full_name : null,
          role: form.role !== user.role ? form.role : null,
          is_active: form.is_active !== user.is_active ? form.is_active : null,
          password: form.password || null,
        });
      } else {
        await api.post("/users", { username: form.username, full_name: form.full_name, password: form.password, role: form.role });
      }
      toastSuccess(t("common.saved"));
      onSaved();
      onClose();
    });
  };

  return (
    <Modal isOpen onClose={onClose} title={user ? user.username : t("users.new")}>
      <form onSubmit={submit}>
        {!user && (
          <Field label={t("auth.username")}>
            <input required dir="ltr" autoComplete="off" pattern="[a-z0-9._\-]{3,64}" maxLength={64} title={t("validation.username")} value={form.username} onChange={(e) => setForm({ ...form, username: e.target.value.toLowerCase().trim() })} />
          </Field>
        )}
        <Field label={t("common.name")}>
          <input required value={form.full_name} onChange={(e) => setForm({ ...form, full_name: e.target.value })} />
        </Field>
        <Field label={t("users.role")} hint={t(`roleHints.${form.role}`)}>
          <select value={form.role} onChange={(e) => setForm({ ...form, role: e.target.value as Role })}>
            {ROLES.map((r) => (
              <option key={r} value={r}>
                {t(`roles.${r}`)}
              </option>
            ))}
          </select>
        </Field>
        <Field label={user ? t("users.resetPassword") : t("auth.password")} hint={t("users.passwordHint", { min: MIN_PASSWORD_LEN })}>
          <input type="password" autoComplete="new-password" required={!user} value={form.password} onChange={(e) => setForm({ ...form, password: e.target.value })} />
        </Field>
        {user && (
          <label className="checkbox-label">
            <input type="checkbox" checked={form.is_active} onChange={(e) => setForm({ ...form, is_active: e.target.checked })} />
            {t("common.active")}
          </label>
        )}
        <FormActions busy={busy || !passwordOk} onClose={onClose} />
      </form>
    </Modal>
  );
}

interface Catalog {
  permissions: string[];
  roles: Record<string, string[]>;
}

/** Grants exactly the ticked permissions, or returns the user to the role's defaults. */
function PermissionsEditor({ user, onClose, onSaved }: { user: User; onClose: () => void; onSaved: () => void }) {
  const { t } = useTranslation();
  const { data: catalog, error } = useLoad(() => api.get<Catalog>("/permissions"));
  const [custom, setCustom] = useState(user.permissions !== null);
  const [picked, setPicked] = useState<Set<string> | null>(user.permissions ? new Set(user.permissions) : null);
  const { busy, run } = useSubmit();
  const defaults = new Set(catalog?.roles[user.role] ?? []);
  const current = custom ? (picked ?? defaults) : defaults;

  const toggle = (names: readonly string[], on: boolean) => {
    const next = new Set(current);
    names.forEach((n) => (on ? next.add(n) : next.delete(n)));
    setPicked(next);
  };

  const save = () =>
    run(async () => {
      await api.patch(`/users/${user.id}`, { permissions: custom ? [...current] : null });
      toastSuccess(t("common.saved"));
      onSaved();
      onClose();
    });

  return (
    <Modal isOpen onClose={onClose} size="lg" title={t("users.permissionsOf", { name: user.full_name })}>
      {error && <ErrorNote message={error} />}
      <label className="checkbox-label">
        <input
          type="checkbox"
          checked={custom}
          onChange={(e) => {
            setCustom(e.target.checked);
            if (e.target.checked && !picked) setPicked(new Set(defaults));
          }}
        />
        {t("users.customize", { role: t(`roles.${user.role}`) })}
      </label>
      <p className="settings-hint">{t("users.permissionsHint")}</p>
      <div className={`perm-grid${custom ? "" : " is-readonly"}`}>
        {(Object.keys(PERMISSION_GROUPS) as PermissionGroup[]).map((g) => {
          const names = PERMISSION_GROUPS[g];
          const all = names.every((n) => current.has(n));
          return (
            <fieldset key={g} className="perm-group" disabled={!custom}>
              <legend>
                <label className="checkbox-label">
                  <input type="checkbox" checked={all} onChange={(e) => toggle(names, e.target.checked)} />
                  <strong>{t(`permGroups.${g}`)}</strong>
                </label>
              </legend>
              {names.map((n) => (
                <label key={n} className="checkbox-label perm-item">
                  <input type="checkbox" checked={current.has(n)} onChange={(e) => toggle([n], e.target.checked)} />
                  <span>{t(`perms.${n.replace(".", "_")}`)}</span>
                  {defaults.has(n) && <small className="perm-default">{t("users.inRole")}</small>}
                </label>
              ))}
            </fieldset>
          );
        })}
      </div>
      <div className="form-actions">
        <button type="button" className="btn btn-primary" disabled={busy || !catalog} onClick={save} data-shortcut="save">
          {t("common.save")}
        </button>
        <button type="button" className="btn btn-secondary" onClick={onClose}>
          {t("common.cancel")}
        </button>
      </div>
    </Modal>
  );
}
