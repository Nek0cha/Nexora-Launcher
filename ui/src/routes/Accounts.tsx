import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { Button, Input, TextField } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconCheck from "~icons/ph/check";
import IconEmpty from "~icons/ph/user-circle";
import { appInfo, getDevOfflineUsername, setDevOfflineUsername } from "../api/commands";
import { toErrorPayload } from "../api/ipc";
import { EmptyState } from "../components/EmptyState";
import { FailureNotice } from "../components/FailureNotice";

const DEV_USERNAME_KEY = ["devOfflineUsername"] as const;

export default function Accounts() {
  const { t } = useTranslation();
  const { data: info } = useQuery({ queryKey: ["appInfo"], queryFn: appInfo });
  return (
    <EmptyState icon={IconEmpty} title={t("nav.accounts")} body={t("empty.accounts")}>
      {/* 製品版の backend にはこの command がない（app.dev_only）ため、開発ビルドのときだけ出す */}
      {info?.build === "dev" && <DevOfflineAccount />}
    </EmptyState>
  );
}

function DevOfflineAccount() {
  const { t } = useTranslation();
  const titleId = useId();
  const { data, error } = useQuery({ queryKey: DEV_USERNAME_KEY, queryFn: getDevOfflineUsername });
  return (
    <section aria-labelledby={titleId} className="flex w-md max-w-full flex-col gap-3 border-t border-line pt-5">
      <div className="flex flex-col gap-1">
        <h2 id={titleId} className="font-medium text-text-strong">
          {t("accounts.devTitle")}
        </h2>
        <p className="text-sm text-text-muted">{t("accounts.devHint")}</p>
      </div>
      {error ? (
        <div role="alert">
          <FailureNotice error={toErrorPayload(error)} />
        </div>
      ) : data === undefined ? (
        // 届いた後の入力の行と同じ高さを空けておく
        <div aria-busy="true" className="h-9" />
      ) : (
        <DevUsernameForm initial={data} labelledBy={titleId} />
      )}
    </section>
  );
}

function DevUsernameForm({ initial, labelledBy }: { initial: string; labelledBy: string }) {
  const { t } = useTranslation();
  const client = useQueryClient();
  const [value, setValue] = useState(initial);
  const [saved, setSaved] = useState(false);
  const save = useMutation({
    mutationFn: (name: string) => setDevOfflineUsername(name),
    onSuccess: (name) => {
      client.setQueryData(DEV_USERNAME_KEY, name);
      setValue(name);
      setSaved(true);
    },
  });

  return (
    <div className="flex flex-col gap-3">
      <form
        className="flex items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (save.isPending) return;
          setSaved(false);
          save.mutate(value);
        }}
      >
        <TextField
          aria-labelledby={labelledBy}
          value={value}
          onChange={(next) => {
            setValue(next);
            setSaved(false);
            // 直し始めたら、前の保存の失敗は消す
            if (save.isError) save.reset();
          }}
          maxLength={16}
        >
          <Input
            spellCheck={false}
            className={[
              "h-9 w-48 rounded-md border border-line-control bg-surface-1 px-3 text-text",
              "transition-colors duration-(--duration-fast) ease-out-quint",
              "data-hovered:border-text-muted",
              "data-focused:outline-2 data-focused:outline-offset-2 data-focused:outline-accent",
            ].join(" ")}
          />
        </TextField>
        <Button
          type="submit"
          isPending={save.isPending}
          className={[
            "inline-flex h-9 items-center rounded-md border border-line-control px-3 text-sm text-text",
            "transition-colors duration-(--duration-fast) ease-out-quint",
            "hover:bg-surface-2 pressed:bg-surface-3 data-pending:cursor-progress",
            "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
          ].join(" ")}
        >
          {t("accounts.save")}
        </Button>
        <span role="status" className="flex items-center gap-1.5 text-sm text-text-muted">
          {saved && (
            <>
              <IconCheck aria-hidden className="size-4 text-accent" />
              {t("accounts.saved")}
            </>
          )}
        </span>
      </form>
      {save.error && (
        <div role="alert">
          <FailureNotice error={toErrorPayload(save.error)} />
        </div>
      )}
    </div>
  );
}
