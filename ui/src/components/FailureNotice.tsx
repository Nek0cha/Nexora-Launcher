import { useId, useState } from "react";
import { Button } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconWarning from "~icons/ph/warning-circle";
import type { ErrorPayload } from "../bindings/ErrorPayload";
import { errorKey } from "../features/errors/errorText";
import i18n from "../i18n";

/** 失敗の説明。タスクの失敗と、開始やキャンセルの呼び出し自体の失敗の両方に使う。 */
export function FailureNotice({ error }: { error: ErrorPayload | null }) {
  const { t } = useTranslation();
  const [showDetail, setShowDetail] = useState(false);
  const detailId = useId();
  return (
    <div className="flex max-w-xl flex-col gap-2">
      <p className="flex items-center gap-2 text-danger">
        <IconWarning aria-hidden className="size-4 shrink-0" />
        <span>
          {error
            ? t(errorKey(error.code, (k) => i18n.exists(k)) as never, {
                ...(error.params as Record<string, unknown>),
                code: error.code,
              })
            : t("errors:generic" as never, { code: "unknown" })}
        </span>
      </p>
      {error?.detail && (
        <Button
          onPress={() => setShowDetail((v) => !v)}
          aria-expanded={showDetail}
          aria-controls={showDetail ? detailId : undefined}
          className="self-start select-none rounded-sm text-sm text-text-muted underline underline-offset-2 data-hovered:text-text data-pressed:text-text-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
        >
          {t("play.details")}
        </Button>
      )}
      {showDetail && error?.detail && (
        <pre
          id={detailId}
          className="tabular max-h-48 overflow-auto whitespace-pre-wrap rounded-md bg-surface-1 p-3 text-xs text-text"
        >
          {error.detail}
        </pre>
      )}
    </div>
  );
}
