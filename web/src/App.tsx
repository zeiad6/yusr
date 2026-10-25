import { useCallback, useEffect, useState, type ReactNode } from "react";
import { api, errorMessage } from "./api/client";
import type { Me } from "./api/types";
import { Navigate, Route, Routes } from "react-router-dom";
import { useAuthStore } from "./store/auth";
import { useRefs } from "./store/refs";
import type { Permission } from "./store/permissions";
import Layout from "./components/Layout";
import { ErrorNote, Loading } from "./components/ui";
import Login from "./pages/Login";
import Dashboard from "./pages/Dashboard";
import Journal from "./pages/Journal";
import Accounts from "./pages/Accounts";
import Funds from "./pages/Funds";
import Donors from "./pages/Donors";
import Donations from "./pages/Donations";
import Beneficiaries from "./pages/Beneficiaries";
import Zakat from "./pages/Zakat";
import Invoices from "./pages/Invoices";
import Audit from "./pages/Audit";
import Users from "./pages/Users";
import Settings from "./pages/Settings";
import PaymentReturn from "./pages/PaymentReturn";
import Reports from "./pages/Reports";
import Archive from "./pages/Archive";
import Letters from "./pages/Letters";
import Sponsorships from "./pages/Sponsorships";
import Volunteers from "./pages/Volunteers";
import { NAV } from "./components/Sidebar";

function Protected({ children }: { children: ReactNode }) {
  const session = useAuthStore((s) => s.session);
  const hasSession = session !== null;
  const setPermissions = useAuthStore((s) => s.setPermissions);
  const { loaded, load } = useRefs();
  const [fresh, setFresh] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Permissions can change while a session is stored, so they are re-read before the
  // reference data, which depends on them.
  const attempt = useCallback(() => {
    setError(null);
    api
      .get<Me>("/auth/me")
      .then((me) => {
        setPermissions(me.permissions);
        setFresh(true);
        return load();
      })
      .catch((e) => setError(errorMessage(e)));
  }, [load, setPermissions]);
  useEffect(() => {
    if (hasSession) attempt();
  }, [hasSession, attempt]);
  if (!session) return <Navigate to="/login" replace />;
  if (error) return <div className="page boot-error"><ErrorNote message={error} onRetry={attempt} /></div>;
  return loaded && fresh ? <>{children}</> : <Loading />;
}

function Guard({ permission, children }: { permission: Permission; children: ReactNode }) {
  const can = useAuthStore((s) => s.can);
  if (can(permission)) return <>{children}</>;
  // Land on the first page this role may open instead of an error screen.
  const first = NAV.find((n) => n.permission === null || can(n.permission));
  return <Navigate to={first?.path === "/" ? "/settings" : (first?.path ?? "/settings")} replace />;
}

export default function App() {
  const g = (permission: Permission, el: ReactNode) => <Guard permission={permission}>{el}</Guard>;
  return (
    <Routes>
      <Route path="/login" element={<Login />} />
      {/* Public: donors land here from the gateway without a staff session. */}
      <Route path="/donations/return" element={<PaymentReturn />} />
      <Route
        path="/"
        element={
          <Protected>
            <Layout />
          </Protected>
        }
      >
        <Route index element={g("reports.read", <Dashboard />)} />
        <Route path="journal" element={g("ledger.read", <Journal />)} />
        <Route path="accounts" element={g("ledger.read", <Accounts />)} />
        <Route path="funds" element={g("reports.read", <Funds />)} />
        <Route path="donors" element={g("donors.read", <Donors />)} />
        <Route path="donations" element={g("donations.read", <Donations />)} />
        <Route path="beneficiaries" element={g("cases.read", <Beneficiaries />)} />
        <Route path="sponsorships" element={g("sponsorships.read", <Sponsorships />)} />
        <Route path="volunteers" element={g("volunteers.read", <Volunteers />)} />
        <Route path="reports" element={g("reports.read", <Reports />)} />
        <Route path="archive" element={g("archive.read", <Archive />)} />
        <Route path="letters" element={g("letters.read", <Letters />)} />
        <Route path="zakat" element={g("zakat.calculate", <Zakat />)} />
        <Route path="invoices" element={g("invoices.read", <Invoices />)} />
        <Route path="audit" element={g("audit.read", <Audit />)} />
        <Route path="users" element={g("users.manage", <Users />)} />
        <Route path="settings" element={<Settings />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}
