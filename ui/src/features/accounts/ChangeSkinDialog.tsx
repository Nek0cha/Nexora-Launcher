import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { Button, Dialog, DialogTrigger, Heading, Label, Modal, ModalOverlay, Radio, RadioGroup } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { changeSkin, getSkinVariant } from "../../api/commands";
import { toErrorPayload } from "../../api/ipc";
import type { SkinVariant } from "../../bindings/SkinVariant";
import { FailureNotice } from "../../components/FailureNotice";
import { modalClass, overlayClass } from "../../components/modal";
import { ACCOUNTS_KEY, SKIN_KEY } from "./useAccounts";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const rowButton = [
  "inline-flex h-8 items-center rounded-md px-2.5 text-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3 data-disabled:opacity-50",
  ring,
].join(" ");

const primaryButton = [
  "inline-flex h-9 items-center justify-center rounded-md bg-text-strong px-4 text-sm font-medium text-surface-0",
  "transition-opacity duration-(--duration-fast) ease-out-quint",
  "data-hovered:opacity-90 data-disabled:opacity-50 data-pending:cursor-progress data-pending:opacity-70",
  ring,
].join(" ");

const secondaryButton = [
  "inline-flex h-9 items-center justify-center rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 pressed:bg-surface-3 data-disabled:opacity-50",
  ring,
].join(" ");

const radioClass = [
  "flex h-9 cursor-default items-center rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "data-hovered:bg-surface-3 data-selected:border-accent data-selected:text-text-strong",
  "data-focus-visible:outline-2 data-focus-visible:outline-offset-2 data-focus-visible:outline-accent",
].join(" ");

/** Microsoft アカウントのスキンの変更。PNG を選び、モデルを選んで送る（spec 4.1）。 */
export function ChangeSkinDialog({ accountId, isDisabled = false }: { accountId: string; isDisabled?: boolean }) {
  const { t } = useTranslation();
  return (
    <DialogTrigger>
      <Button isDisabled={isDisabled} className={rowButton}>
        {t("accounts.changeSkin")}
      </Button>
      <ModalOverlay className={overlayClass}>
        <Modal className={modalClass}>
          <Dialog className="outline-hidden">
            {({ close }) => <ChangeSkinContent accountId={accountId} close={close} />}
          </Dialog>
        </Modal>
      </ModalOverlay>
    </DialogTrigger>
  );
}

/** ダイアログの中身。開くたびに作り直されるため、選んだファイルは毎回まっさらから始まる。 */
function ChangeSkinContent({ accountId, close }: { accountId: string; close: () => void }) {
  const { t } = useTranslation();
  const client = useQueryClient();
  const fileId = useId();
  const [file, setFile] = useState<File | null>(null);
  // 利用者が選ぶまでは null。選ぶまでは現在のモデルを初期値にする
  const [chosen, setChosen] = useState<SkinVariant | null>(null);

  // 現在のモデル。取れない間と失敗したときは Classic にする
  const current = useQuery({
    queryKey: ["skinVariant", accountId],
    queryFn: () => getSkinVariant(accountId),
    gcTime: 0,
    retry: false,
  });
  const variant: SkinVariant = chosen ?? current.data ?? "classic";

  const submit = useMutation({
    mutationFn: async () => {
      if (!file) throw new Error("no file");
      const buf = await file.arrayBuffer();
      await changeSkin(accountId, variant, Array.from(new Uint8Array(buf)));
    },
    onSuccess: () => {
      // 顔の画像は staleTime が無限のため、無効にして取り直させる。所有の確認の時刻は変わらない
      void client.invalidateQueries({ queryKey: [...SKIN_KEY, accountId] });
      void client.invalidateQueries({ queryKey: ACCOUNTS_KEY });
      close();
    },
  });

  return (
    <form
      className="flex flex-col gap-4 p-5"
      onSubmit={(e) => {
        e.preventDefault();
        if (file && !submit.isPending) submit.mutate();
      }}
    >
      <Heading slot="title" className="font-display text-lg font-semibold text-text-strong">
        {t("accounts.skinTitle")}
      </Heading>
      <div className="flex flex-col gap-1.5">
        <label htmlFor={fileId} className="text-sm text-text">
          {t("accounts.skinFile")}
        </label>
        <input
          id={fileId}
          type="file"
          accept="image/png"
          onChange={(e) => {
            setFile(e.target.files?.[0] ?? null);
            if (submit.isError) submit.reset();
          }}
          className="text-sm text-text-muted file:mr-3 file:h-8 file:rounded-md file:border file:border-line-control file:bg-transparent file:px-3 file:text-text"
        />
        <p className="text-sm text-text-muted">{t("accounts.skinFileHint")}</p>
      </div>
      <RadioGroup
        value={variant}
        onChange={(v) => setChosen(v as SkinVariant)}
        orientation="horizontal"
        className="flex flex-col gap-1.5"
      >
        <Label className="text-sm text-text">{t("accounts.skinModel")}</Label>
        <div className="flex gap-2">
          <Radio value="classic" className={radioClass}>
            {t("accounts.skinClassic")}
          </Radio>
          <Radio value="slim" className={radioClass}>
            {t("accounts.skinSlim")}
          </Radio>
        </div>
      </RadioGroup>
      {submit.error && (
        <div role="alert">
          <FailureNotice error={toErrorPayload(submit.error)} />
        </div>
      )}
      <div className="flex justify-end gap-2 pt-1">
        <Button onPress={close} isDisabled={submit.isPending} className={secondaryButton}>
          {t("accounts.cancel")}
        </Button>
        <Button type="submit" isDisabled={!file} isPending={submit.isPending} className={primaryButton}>
          {t("accounts.skinSubmit")}
        </Button>
      </div>
    </form>
  );
}
