import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { Button, Checkbox, Dialog, Heading, Modal, ModalOverlay, Tooltip, TooltipTrigger } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconUpgrade from "~icons/ph/arrow-circle-up";
import IconCheck from "~icons/ph/check";
import IconQuestion from "~icons/ph/question";
import IconWarning from "~icons/ph/warning";
import { previewUpgrade, upgradeInstance } from "../../api/commands";
import { toErrorPayload } from "../../api/ipc";
import type { LoaderKind } from "../../bindings/LoaderKind";
import { loaderGameVersionsQuery, versionsQuery } from "../../features/instances/useCreateInstance";
import { INSTANCES_KEY } from "../../features/instances/useInstances";
import { upgradeTargets } from "../../features/upgrade/upgradeVersions";
import { FailureNotice } from "../FailureNotice";
import { modalClass, overlayClass } from "../modal";
import { checkboxClass, checkboxMark, onPanel, ring, secondaryButton, submitButton } from "../mrpack/dialogStyles";
import { Select } from "../Select";
import { Skeleton } from "../Skeleton";
import { tooltipClass } from "../tooltip";

const iconButton = [
  "inline-flex size-8 items-center justify-center rounded-md text-text-muted",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-2 hover:text-text pressed:bg-surface-3 data-disabled:opacity-50",
  ring,
].join(" ");

/** インスタンスの詳細の見出しに置く、Minecraft のバージョンを上げる操作（アイコンのボタンとダイアログ）。 */
export function UpgradeInstanceDialog({
  instanceId,
  mcVersion,
  loader,
  isDisabled = false,
}: {
  instanceId: string;
  mcVersion: string;
  loader: LoaderKind;
  isDisabled?: boolean;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  return (
    <>
      <TooltipTrigger delay={500}>
        <Button
          aria-label={t("upgrade.open")}
          isDisabled={isDisabled}
          onPress={() => setOpen(true)}
          className={iconButton}
        >
          <IconUpgrade aria-hidden className="size-4" />
        </Button>
        <Tooltip offset={6} className={tooltipClass}>
          {t("upgrade.open")}
        </Tooltip>
      </TooltipTrigger>
      <ModalOverlay isOpen={open} onOpenChange={setOpen} isDismissable className={overlayClass}>
        <Modal className={modalClass}>
          <Dialog className="outline-hidden">
            <UpgradeForm instanceId={instanceId} mcVersion={mcVersion} loader={loader} onDone={() => setOpen(false)} />
          </Dialog>
        </Modal>
      </ModalOverlay>
    </>
  );
}

/** ファイルのパス（`mods/a.jar`）から名前だけを取り出す。 */
const fileName = (relPath: string) => relPath.slice(relPath.lastIndexOf("/") + 1);

function UpgradeForm({
  instanceId,
  mcVersion,
  loader,
  onDone,
}: {
  instanceId: string;
  mcVersion: string;
  loader: LoaderKind;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const client = useQueryClient();
  const versions = useQuery(versionsQuery());
  const loaderVersions = useQuery({ ...loaderGameVersionsQuery(loader), enabled: loader !== "vanilla" });
  const loading = versions.isPending || (loader !== "vanilla" && loaderVersions.isPending);

  const targets = useMemo(
    () =>
      upgradeTargets(
        mcVersion,
        loader,
        versions.data?.map((v) => v.id) ?? [],
        loader === "vanilla" ? undefined : loaderVersions.data,
      ),
    [mcVersion, loader, versions.data, loaderVersions.data],
  );
  const [chosen, setChosen] = useState<string | null>(null);
  // 選んだバージョンが一覧から外れたら、いちばん新しいバージョンに戻す
  const target = chosen !== null && targets.includes(chosen) ? chosen : (targets[0] ?? null);

  // 開くたびに作り直されるので、そのつど MOD の対応を調べ直す
  const preview = useQuery({
    queryKey: ["upgrade-preview", instanceId, target],
    queryFn: () => previewUpgrade(instanceId, target as string),
    enabled: target !== null,
    retry: false,
    gcTime: 0,
    staleTime: 0,
  });
  const [skipCheck, setSkipCheck] = useState(false);
  const ready = target !== null && (preview.data !== undefined || (preview.isError && skipCheck));

  const run = useMutation({
    mutationFn: () => upgradeInstance(instanceId, target as string),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: INSTANCES_KEY });
      onDone();
    },
  });

  const listError = versions.error ?? loaderVersions.error;

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (ready && !run.isPending) run.mutate();
      }}
    >
      <Heading slot="title" className="px-5 pt-5 font-display text-lg font-semibold text-text-strong">
        {t("upgrade.title")}
      </Heading>
      <div className="flex flex-col gap-3 px-5 pt-4 pb-5">
        {loading && !listError && (
          <div aria-busy="true" className="flex h-9 items-center justify-between gap-4">
            <span role="status" className="sr-only">
              {t("upgrade.loading")}
            </span>
            <Skeleton className={`h-5 w-20 ${onPanel}`} />
            <Skeleton className={`h-9 w-48 ${onPanel}`} />
          </div>
        )}
        {listError && (
          <div role="alert">
            <FailureNotice error={toErrorPayload(listError)} />
          </div>
        )}
        {!loading && !listError && targets.length === 0 && (
          <p className="flex items-center gap-2 text-sm text-text-muted">
            <IconWarning aria-hidden className="size-4 shrink-0" />
            <span>{t("upgrade.none")}</span>
          </p>
        )}
        {target !== null && (
          <>
            <Select
              label={t("upgrade.target")}
              value={target}
              options={targets.map((id) => ({ id, label: id }))}
              onChange={(id) => {
                setChosen(id);
                setSkipCheck(false);
              }}
            />
            {preview.isPending && (
              <div aria-busy="true" className="flex flex-col gap-2">
                <span role="status" className="sr-only">
                  {t("upgrade.checking")}
                </span>
                <Skeleton className={`h-6 w-full ${onPanel}`} />
              </div>
            )}
            {preview.error && (
              <div role="alert" className="flex flex-col gap-2">
                <FailureNotice error={toErrorPayload(preview.error)} />
                {/* 使えないバージョンと分かった場合は、確認を省いても上げられない */}
                {toErrorPayload(preview.error).code !== "upgrade.unavailable" && (
                  <Checkbox isSelected={skipCheck} onChange={setSkipCheck} className={checkboxClass}>
                    <span aria-hidden className={checkboxMark}>
                      <IconCheck className="size-3 opacity-0 group-data-selected:opacity-100" />
                    </span>
                    {t("upgrade.skip")}
                  </Checkbox>
                )}
              </div>
            )}
            {preview.data && (
              <div className="flex flex-col gap-3">
                <div className="flex flex-wrap items-center gap-4">
                  <Count
                    icon={IconCheck}
                    tone="text-accent"
                    label={t("upgrade.compatible")}
                    count={preview.data.compatible.length}
                  />
                  <Count
                    icon={IconWarning}
                    tone="text-warning"
                    label={t("upgrade.incompatible")}
                    count={preview.data.incompatible.length}
                  />
                  <Count
                    icon={IconQuestion}
                    tone="text-text-muted"
                    label={t("upgrade.unknown")}
                    count={preview.data.unknown.length}
                  />
                </div>
                <Names icon={IconWarning} tone="text-warning" paths={preview.data.incompatible} />
                <Names icon={IconQuestion} tone="text-text-muted" paths={preview.data.unknown} />
              </div>
            )}
          </>
        )}
        {run.error && (
          <div role="alert">
            <FailureNotice error={toErrorPayload(run.error)} />
          </div>
        )}
      </div>
      <div className="flex justify-end gap-2 border-t border-line px-5 py-3">
        <Button slot="close" className={secondaryButton}>
          {t("create.cancel")}
        </Button>
        <Button type="submit" isDisabled={!ready} isPending={run.isPending} className={submitButton}>
          {t("upgrade.submit")}
        </Button>
      </div>
    </form>
  );
}

type Icon = typeof IconCheck;

/** 件数のバッジ。名前は「ラベル: 件数」。 */
function Count({ icon: Icon, tone, label, count }: { icon: Icon; tone: string; label: string; count: number }) {
  return (
    <span aria-label={`${label}: ${count}`} title={label} className="tabular inline-flex items-center gap-1.5 text-sm text-text">
      <Icon aria-hidden className={`size-4 ${tone}`} />
      <span aria-hidden>{count}</span>
    </span>
  );
}

/** 気をつけたい MOD のファイル名の一覧。空なら何も出さない。 */
function Names({ icon: Icon, tone, paths }: { icon: Icon; tone: string; paths: string[] }) {
  if (paths.length === 0) return null;
  return (
    <ul className="scroll-thin flex max-h-32 flex-col gap-0.5 overflow-y-auto text-sm text-text">
      {paths.map((path) => (
        <li key={path} className="flex items-center gap-2 truncate">
          <Icon aria-hidden className={`size-4 shrink-0 ${tone}`} />
          <span className="truncate">{fileName(path)}</span>
        </li>
      ))}
    </ul>
  );
}
