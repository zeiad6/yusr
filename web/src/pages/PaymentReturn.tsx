import { Link, useSearchParams } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { CheckCircle2 } from "lucide-react";
import { useAuthStore } from "../store/auth";

/** Landing page after a hosted checkout. The donation is recorded only by the gateway's signed webhook. */
export default function PaymentReturn() {
  const { t } = useTranslation();
  const [params] = useSearchParams();
  const signedIn = useAuthStore((s) => s.session !== null);
  return (
    <div className="login-page">
      <div className="chart-card payment-return">
        <CheckCircle2 size={40} aria-hidden />
        <h2>{t("payment.title")}</h2>
        <p>{t("payment.body")}</p>
        {params.get("intent") && <code className="hash">{params.get("intent")}</code>}
        {signedIn && (
          <Link className="btn btn-primary" to="/donations">
            {t("nav.donations")}
          </Link>
        )}
      </div>
    </div>
  );
}
