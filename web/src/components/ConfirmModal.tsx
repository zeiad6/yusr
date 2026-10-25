import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { AlertTriangle, AlertCircle, Info, X } from "lucide-react";

export interface ConfirmModalProps {
  isOpen: boolean;
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  variant?: "danger" | "warning" | "primary";
  onConfirm: () => Promise<void> | void;
  onClose: () => void;
}

export default function ConfirmModal({
  isOpen,
  title,
  message,
  confirmText = "تأكيد",
  cancelText = "إلغاء",
  variant = "danger",
  onConfirm,
  onClose,
}: ConfirmModalProps) {
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && isOpen && !loading) {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, loading, onClose]);

  if (!isOpen) return null;

  const handleConfirm = async () => {
    try {
      setLoading(true);
      await onConfirm();
      onClose();
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  const getVariantConfig = () => {
    switch (variant) {
      case "danger":
        return {
          icon: <AlertTriangle size={24} />,
          btnClass: "btn-confirm-danger",
        };
      case "warning":
        return {
          icon: <AlertCircle size={24} />,
          btnClass: "btn-confirm-warning",
        };
      case "primary":
      default:
        return {
          icon: <Info size={24} />,
          btnClass: "btn-primary",
        };
    }
  };

  const config = getVariantConfig();

  return createPortal(
    <div
      className="modal-overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-dialog-title"
      onClick={(e) => {
        if (!loading && e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="modal modal-confirm"
        onClick={(e) => e.stopPropagation()}
      >
        <button
          type="button"
          onClick={onClose}
          disabled={loading}
          className="modal-confirm-close"
          aria-label="إغلاق"
        >
          <X size={18} />
        </button>

        <div className="modal-confirm-head">
          <div className={`modal-confirm-icon ${variant}`}>
            {config.icon}
          </div>
          <div className="modal-confirm-body">
            <h3 id="confirm-dialog-title" className="modal-confirm-title">
              {title}
            </h3>
            <p className="modal-confirm-message">
              {message}
            </p>
          </div>
        </div>

        <div className="modal-confirm-actions">
          <button
            type="button"
            onClick={onClose}
            disabled={loading}
            className="btn btn-secondary"
          >
            {cancelText}
          </button>
          <button
            type="button"
            onClick={handleConfirm}
            disabled={loading}
            className={`btn ${config.btnClass} ${loading ? "is-loading" : ""}`}
          >
            {loading ? "جاري التنفيذ..." : confirmText}
          </button>
        </div>
      </div>
    </div>,
    document.body
  );
}
