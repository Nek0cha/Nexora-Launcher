import { useEffect, useState } from "react";
import { Button, Dialog, DialogTrigger, Heading, Input, Label, Modal, ModalOverlay, TextField } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconPlus from "~icons/ph/plus";
import IconRetry from "~icons/ph/arrow-clockwise";
import { toErrorPayload } from "../api/ipc";
import { nextName } from "../features/instances/createName";
import { useCreateInstance, useVersions } from "../features/instances/useCreateInstance";
import { FailureNotice } from "./FailureNotice";
import { modalClass, overlayClass } from "./modal";
import { Select } from "./Select";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// 空の状態の主な操作。プレイボタンと同じ明暗の反転の塗りと、押下の縮み
const primaryTrigger = [
  "play-press inline-flex h-11 items-center gap-2 rounded-lg bg-text-strong px-5",
  "font-display font-semibold text-surface-0 data-hovered:opacity-90",
  ring,
].join(" ");

// 見出しの横に置く控えめな操作。面は乗せたときだけ現れる
const quietTrigger = [
  "inline-flex h-8 items-center gap-1.5 rounded-md px-2.5 text-sm text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3",
  ring,
].join(" ");

const secondaryButton = [
  "inline-flex h-9 items-center gap-2 rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 pressed:bg-surface-3",
  ring,
].join(" ");

const submitButton = [
  "inline-flex h-9 items-center gap-2 rounded-md bg-text-strong px-4 text-sm font-medium text-surface-0",
  "transition-opacity duration-(--duration-fast) ease-out-quint",
  "data-hovered:opacity-90 data-disabled:opacity-50 data-pending:cursor-progress data-pending:opacity-70",
  ring,
].join(" ");

// 右の列の幅。名前の欄、バージョンの選択欄、読み込み中の表示で揃える（日本語の「読み込んでいます」が 1 行に収まる幅）
const controlWidth = "w-60";

// 名前の欄は、Select と同じ「ラベルが左、入力が右」の行にし、右端を揃える
const inputClass = [
  `h-9 ${controlWidth} rounded-md border border-line-control bg-surface-1 px-3 text-text`,
  "placeholder:text-text-faint",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "data-hovered:border-text-muted",
  "data-focused:outline-2 data-focused:outline-offset-2 data-focused:outline-accent",
].join(" ");

export function CreateInstanceDialog({ trigger }: { trigger: "primary" | "quiet" }) {
  const { t } = useTranslation();
  return (
    <DialogTrigger>
      <Button className={trigger === "primary" ? primaryTrigger : quietTrigger}>
        <IconPlus aria-hidden className={trigger === "primary" ? "size-5" : "size-4"} />
        <span>{t("create.open")}</span>
      </Button>
      <ModalOverlay isDismissable className={overlayClass}>
        <Modal className={modalClass}>
          <Dialog className="outline-hidden">{({ close }) => <CreateForm onDone={close} />}</Dialog>
        </Modal>
      </ModalOverlay>
    </DialogTrigger>
  );
}

/** ダイアログの中身。開くたびに作り直されるため、入力は毎回まっさらから始まる。 */
function CreateForm({ onDone }: { onDone: () => void }) {
  const { t } = useTranslation();
  const versions = useVersions(true);
  const create = useCreateInstance();
  const [version, setVersion] = useState<string | null>(null);
  const [name, setName] = useState("");

  // 一覧が届いたら、最初の項目（最新の正式版）を選ぶ
  const first = versions.data?.[0]?.id;
  useEffect(() => {
    if (version !== null || first === undefined) return;
    setVersion(first);
    setName((current) => nextName(current, null, first));
  }, [first, version]);

  const submit = () => {
    if (version === null || create.isPending) return;
    create.mutate({ name, mcVersion: version }, { onSuccess: onDone });
  };

  return (
    <form
      className="flex flex-col"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <Heading slot="title" className="px-5 pt-5 font-display text-lg font-semibold text-text-strong">
        {t("create.title")}
      </Heading>
      <div className="flex flex-col gap-3 px-5 pt-4 pb-5">
        <TextField
          value={name}
          onChange={setName}
          maxLength={64}
          autoFocus
          className="flex items-center justify-between gap-4"
        >
          <Label className="text-text">{t("create.name")}</Label>
          {/* 空欄のまま作ると、backend がバージョン名を使う。そのことを入力例として見せる */}
          <Input placeholder={version ?? undefined} className={inputClass} />
        </TextField>
        <VersionRow
          state={versions}
          version={version}
          onChange={(next) => {
            setName((current) => nextName(current, version, next));
            setVersion(next);
          }}
        />
        {create.error && (
          <div role="alert" className="pt-1">
            <FailureNotice error={toErrorPayload(create.error)} />
          </div>
        )}
      </div>
      <div className="flex justify-end gap-2 border-t border-line px-5 py-3">
        <Button slot="close" className={secondaryButton}>
          {t("create.cancel")}
        </Button>
        <Button type="submit" isDisabled={version === null} isPending={create.isPending} className={submitButton}>
          {t("create.submit")}
        </Button>
      </div>
    </form>
  );
}

function VersionRow({
  state,
  version,
  onChange,
}: {
  state: ReturnType<typeof useVersions>;
  version: string | null;
  onChange: (version: string) => void;
}) {
  const { t } = useTranslation();
  if (state.error) {
    // 行の形は残し、右の列に再試行を置く。何が失敗したかは行の下に出す
    return (
      <div role="alert" className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-4">
          <span className="text-text">{t("create.version")}</span>
          <Button onPress={() => void state.refetch()} isPending={state.isFetching} className={`${secondaryButton} ${controlWidth} justify-center`}>
            <IconRetry aria-hidden className="size-4" />
            <span>{t("create.retry")}</span>
          </Button>
        </div>
        <FailureNotice error={toErrorPayload(state.error)} />
      </div>
    );
  }
  if (!state.data || version === null) {
    // 届いた後の Select と同じ行の形と高さにして、置き換わってもレイアウトを動かさない
    return (
      <div aria-busy="true" className="flex items-center justify-between gap-4">
        <span className="text-text">{t("create.version")}</span>
        <span className={`flex h-9 ${controlWidth} items-center rounded-md border border-line bg-surface-1 px-3 text-sm text-text-muted`}>
          {t("create.loadingVersions")}
        </span>
      </div>
    );
  }
  return (
    <Select
      label={t("create.version")}
      value={version}
      options={state.data.map((v) => ({ id: v.id, label: v.id }))}
      onChange={onChange}
      triggerClassName={controlWidth}
    />
  );
}
