import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import {
  Button,
  Dialog,
  Heading,
  Input,
  Label,
  Modal,
  ModalOverlay,
  Radio,
  RadioGroup,
  TextField,
} from "react-aria-components";
import { useTranslation } from "react-i18next";
import { createSyncProfileFromInstance, joinSyncProfile, previewJoinSyncProfile } from "../../api/commands";
import { toErrorPayload } from "../../api/ipc";
import type { Applicability } from "../../bindings/Applicability";
import type { JoinMode } from "../../bindings/JoinMode";
import { applicabilityKey, defaultProfileName } from "../../features/sync/syncText";
import { SYNC_KEY, useSyncProfiles } from "../../features/sync/useInstanceSync";
import { FailureNotice } from "../FailureNotice";
import { modalClass, overlayClass } from "../modal";
import { RadioMark } from "../RadioMark";
import { inputBase, Notice } from "../settingsLayout";
import { StableLabel } from "../StableLabel";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

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

// 丸印の付いた選択肢。文言が長いので、印と文字を横に並べて折り返す
const radioClass = [
  "group flex cursor-default items-start gap-2.5 rounded-md py-1.5 text-sm text-text",
  "data-disabled:cursor-not-allowed data-disabled:text-text-faint",
  "data-focus-visible:outline-2 data-focus-visible:outline-offset-2 data-focus-visible:outline-accent",
].join(" ");

function Choice({ value, label, isDisabled, describedBy }: {
  value: string;
  label: string;
  isDisabled?: boolean;
  describedBy?: string;
}) {
  return (
    <Radio value={value} isDisabled={isDisabled} aria-describedby={describedBy} className={radioClass}>
      <RadioMark className="mt-0.5" />
      <span>{label}</span>
    </Radio>
  );
}

type Kind = "create" | "join";

type JoinOptions = {
  /** インスタンスの名前。共通設定の画面から開くときに、どのインスタンスの話かを見出しで示す */
  instanceName?: string;
  /**
   * 共通設定を適用できる状態か。省略すると ready とみなす。
   * 設定ファイルが無くても ready なら、起動前に設定ファイルを作って反映できる。
   */
  applicability?: Applicability;
  /**
   * 参加先と値の違いの件数が決まっているとき（共通設定の画面で、違いがあると分かった後）。
   * どちらに揃えるかだけを聞き、「キャンセル」で閉じる。
   */
  target?: { profileId: string; differing: number };
  /** 参加したとき（新しく作ったときは呼ばない） */
  onJoined?: (profileId: string) => void;
};

/**
 * 「他のインスタンスと設定を揃える」のダイアログ。新しい共通設定を作るか、既存のものに参加するかを選ぶ（spec 3.1、3.2）。
 * 参加しても、その場では設定ファイルを書き換えない。
 */
export function JoinSyncDialog(props: {
  instanceId: string;
  /** 設定ファイルがあるか。無ければ、このインスタンスからは作れない */
  hasOptions: boolean;
  running: boolean;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
} & JoinOptions) {
  return (
    <ModalOverlay isOpen={props.isOpen} onOpenChange={props.onOpenChange} isDismissable className={overlayClass}>
      <Modal className={modalClass}>
        <Dialog className="outline-hidden">
          {/* 閉じると中身が外れるため、開くたびに最初の状態から始まる */}
          <JoinContent {...props} close={() => props.onOpenChange(false)} />
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}

function JoinContent({
  instanceId,
  hasOptions,
  running,
  close,
  instanceName,
  applicability = "ready",
  target,
  onJoined,
}: {
  instanceId: string;
  hasOptions: boolean;
  running: boolean;
  close: () => void;
} & JoinOptions) {
  const { t } = useTranslation();
  const client = useQueryClient();
  const reasonId = useId();
  const profiles = useSyncProfiles().data ?? [];

  // 利用者が選ぶまでは null。選ぶまでは、選べるものから既定を決める
  const [kind, setKind] = useState<Kind | null>(target ? "join" : null);
  const [name, setName] = useState<string | null>(null);
  const [profileId, setProfileId] = useState<string | null>(target?.profileId ?? null);
  // 値の違いがあったときだけ、どちらに揃えるかを聞く
  const [differing, setDiffering] = useState<number | null>(target?.differing ?? null);
  const [mode, setMode] = useState<JoinMode>("keepProfile");

  const chosenKind: Kind | null = kind ?? (hasOptions ? "create" : profiles.length > 0 ? "join" : null);
  const chosenName = name ?? defaultProfileName(t("sync.join.defaultName"), profiles.map((p) => p.name));
  const chosenProfile = profiles.find((p) => p.id === profileId) ?? (target ? undefined : profiles[0]);
  const joinId = chosenProfile?.id ?? target?.profileId;

  const done = () => {
    void client.invalidateQueries({ queryKey: SYNC_KEY });
    close();
  };

  const create = useMutation({
    mutationKey: SYNC_KEY,
    mutationFn: (profileName: string) => createSyncProfileFromInstance(instanceId, profileName),
    onSuccess: done,
  });

  const join = useMutation({
    mutationKey: SYNC_KEY,
    mutationFn: async ({ target, asked }: { target: string; asked: boolean }) => {
      if (!asked) {
        const count = await previewJoinSyncProfile(instanceId, target);
        if (count > 0) return count;
      }
      await joinSyncProfile(instanceId, target, asked ? mode : "keepProfile");
      return 0;
    },
    onSuccess: (count, { target: joined }) => {
      if (count > 0) setDiffering(count);
      else {
        onJoined?.(joined);
        done();
      }
    },
  });

  const pending = create.isPending || join.isPending;
  const error = create.error ?? join.error;
  const asking = differing !== null;
  const canSubmit =
    !pending &&
    (asking ||
      (chosenKind === "create" && hasOptions && chosenName.trim() !== "") ||
      (chosenKind === "join" && joinId !== undefined));

  const submit = () => {
    if (!canSubmit) return;
    if (chosenKind === "create" && !asking) create.mutate(chosenName.trim());
    else if (joinId) join.mutate({ target: joinId, asked: asking });
  };

  const resetErrors = () => {
    create.reset();
    join.reset();
  };

  return (
    <form
      className="flex flex-col gap-4 p-5"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <Heading slot="title" className="font-display text-lg font-semibold text-text-strong">
        {instanceName ? t("sync.join.titleFor", { name: instanceName }) : t("sync.join.title")}
      </Heading>

      {asking ? (
        <RadioGroup
          value={mode}
          onChange={(v) => setMode(v as JoinMode)}
          isDisabled={pending}
          className="flex flex-col gap-2"
        >
          <Label className="text-sm text-text">{t("sync.join.differing", { count: differing })}</Label>
          <div className="flex flex-col">
            <Choice value="keepProfile" label={t("sync.join.keepProfile")} />
            <Choice value="adoptInstance" label={t("sync.join.adoptInstance")} />
          </div>
        </RadioGroup>
      ) : (
        <div className="flex flex-col">
          <RadioGroup
            aria-label={t("sync.join.title")}
            value={chosenKind}
            onChange={(v) => {
              setKind(v as Kind);
              resetErrors();
            }}
            isDisabled={pending}
            className="flex flex-col gap-1"
          >
            <div className="flex flex-col">
              <Choice
                value="create"
                label={t("sync.join.create")}
                isDisabled={!hasOptions}
                describedBy={hasOptions ? undefined : reasonId}
              />
              {!hasOptions && (
                <p id={reasonId} className="pl-6.5 text-sm text-text-muted">
                  {t("sync.join.createDisabled")}
                </p>
              )}
            </div>
            {profiles.length > 0 && <Choice value="join" label={t("sync.join.existing")} />}
          </RadioGroup>
          {/* 選んだものの詳しい入力は、選択肢の下にまとめて置く。
              組の中に置くと、矢印キーが選択肢の移動に使われ、名前の欄で文字の間を動けない */}
          {chosenKind === "create" && hasOptions && (
            <TextField
              value={chosenName}
              onChange={setName}
              maxLength={64}
              isDisabled={pending}
              className="flex flex-col gap-1.5 pt-3"
            >
              <Label className="text-sm text-text-muted">{t("sync.join.name")}</Label>
              <Input className={`${inputBase} w-full`} />
            </TextField>
          )}
          {chosenKind === "join" && profiles.length > 0 && (
            <RadioGroup
              value={chosenProfile?.id ?? null}
              onChange={(v) => {
                setProfileId(v);
                resetErrors();
              }}
              isDisabled={pending}
              className="flex flex-col gap-1 pt-3"
            >
              <Label className="text-sm text-text-muted">{t("sync.join.profile")}</Label>
              <div className="flex flex-col">
                {profiles.map((p) => (
                  <Choice key={p.id} value={p.id} label={p.name} />
                ))}
              </div>
            </RadioGroup>
          )}
        </div>
      )}

      {/* 参加しても、その場では何も書き換えない。選べるものが無いときは何も約束しない。
          適用できない状態なら、反映の時期を約束せずに理由を出す。ただし作れない理由として
          同じ「一度起動すると」を既に出しているときは重ねない */}
      {chosenKind !== null &&
        (applicability === "ready" ? (
          <Notice tone="info">{running ? t("sync.card.afterPlay") : t("sync.card.nextLaunch")}</Notice>
        ) : (
          !(applicability === "needsFirstLaunch" && !hasOptions) && (
            <Notice tone="info">{t(applicabilityKey(applicability))}</Notice>
          )
        ))}

      {error && (
        <div role="alert">
          <FailureNotice error={toErrorPayload(error)} />
        </div>
      )}

      <div className="flex justify-end gap-2 pt-1">
        <Button
          onPress={() => {
            if (asking && !target) {
              setDiffering(null);
              resetErrors();
            } else close();
          }}
          isDisabled={pending}
          className={secondaryButton}
        >
          <StableLabel labels={[t("sync.join.cancel"), t("sync.join.back")]} active={asking && !target ? 1 : 0} />
        </Button>
        <Button type="submit" isDisabled={!canSubmit} isPending={pending} className={primaryButton}>
          <StableLabel
            labels={[t("sync.join.submitCreate"), t("sync.join.submitJoin")]}
            active={chosenKind === "join" || asking ? 1 : 0}
          />
        </Button>
      </div>
    </form>
  );
}
