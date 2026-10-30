import { type ReactNode, useRef } from "react";
import { Button, Dialog, Heading, Input, Modal, ModalOverlay, Radio, RadioGroup, TextField } from "react-aria-components";
import { useTranslation } from "react-i18next";
import IconFile from "~icons/ph/file-image";
import { IpcError } from "../../api/ipc";
import type { SkinVariant } from "../../bindings/SkinVariant";
import { MAX_SKIN_BYTES } from "../../features/skins/useSkins";
import { RadioMark } from "../RadioMark";
import { centeredOverlayClass, modalClass } from "../modal";

const ring = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

const secondaryButton = [
  "inline-flex h-9 items-center gap-2 rounded-md border border-line-control px-3 text-sm text-text",
  "transition-colors duration-(--duration-fast) ease-out-quint",
  "hover:bg-surface-3 pressed:bg-surface-3 data-disabled:opacity-50",
  ring,
].join(" ");

const saveButton = [
  "inline-flex h-9 items-center rounded-md bg-text-strong px-4 text-sm font-medium text-surface-0",
  "transition-opacity duration-(--duration-fast) ease-out-quint",
  "data-hovered:opacity-90 data-pending:cursor-progress data-pending:opacity-70 data-disabled:opacity-50",
  ring,
].join(" ");

const textInput = [
  "h-9 w-full min-w-0 rounded-md border border-line-control bg-surface-1 px-2 text-sm text-text",
  "data-focused:outline-2 data-focused:outline-offset-2 data-focused:outline-accent",
].join(" ");

const label = "text-sm text-text-muted";

/** 画像の中身を本体へ渡す形（バイト列）にする。読む前に大きさを見て、本体の検査と同じ失敗にする */
export async function readSkinFile(file: File): Promise<number[]> {
  if (file.size > MAX_SKIN_BYTES) throw new IpcError({ code: "skin.invalid", params: {}, detail: null });
  return Array.from(new Uint8Array(await file.arrayBuffer()));
}

/** 拡張子を除いたファイル名（スキンの名前にする） */
export const stem = (name: string) => name.replace(/\.[^.]*$/, "") || name;

/**
 * スキンを編集・追加するポップアップの枠。保存を押すまでは何も送らない。
 * 保存の最中は閉じられない。footerStart は左端に置く操作（削除など）。
 */
export function SkinDialog(props: {
  title: string;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  isPending: boolean;
  canSave: boolean;
  onSave: () => void;
  footerStart?: ReactNode;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const { isPending } = props;
  return (
    <ModalOverlay
      isOpen={props.isOpen}
      onOpenChange={(open) => {
        if (!open && isPending) return;
        props.onOpenChange(open);
      }}
      isDismissable={!isPending}
      isKeyboardDismissDisabled={isPending}
      className={centeredOverlayClass}
    >
      <Modal className={`${modalClass} max-w-sm`}>
        <Dialog className="flex flex-col gap-4 p-5 outline-hidden">
          <Heading slot="title" className="font-display text-lg font-semibold text-text-strong">
            {props.title}
          </Heading>
          <form
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault();
              if (props.canSave && !isPending) props.onSave();
            }}
          >
            {props.children}
            <div className="flex items-center gap-2 pt-1">
              {props.footerStart}
              <div className="ml-auto flex gap-2">
                <Button slot="close" isDisabled={isPending} className={secondaryButton}>
                  {t("skins.cancel")}
                </Button>
                <Button type="submit" isDisabled={!props.canSave} isPending={isPending} className={saveButton}>
                  {t("skins.save")}
                </Button>
              </div>
            </div>
          </form>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}

/** 名前、モデル、ファイルの選択。ファイルを選んだことは fileName で見せる（無ければ未選択の文言） */
export function SkinFields(props: {
  name: string;
  onName: (name: string) => void;
  variant: SkinVariant;
  onVariant: (variant: SkinVariant) => void;
  fileName: string | null;
  onFile: (file: File) => void;
  fileLabel: string;
  disabled: boolean;
}) {
  const { t } = useTranslation();
  const picker = useRef<HTMLInputElement>(null);
  const variants: SkinVariant[] = ["classic", "slim"];
  return (
    <>
      <TextField aria-label={t("skins.name")} value={props.name} onChange={props.onName} maxLength={64} isDisabled={props.disabled} autoFocus className="flex flex-col gap-1.5">
        <span aria-hidden className={label}>{t("skins.name")}</span>
        <Input className={textInput} onFocus={(e) => e.currentTarget.select()} />
      </TextField>
      <RadioGroup
        aria-label={t("skins.model")}
        value={props.variant}
        onChange={(v) => props.onVariant(v as SkinVariant)}
        isDisabled={props.disabled}
        className="flex flex-col gap-1.5"
      >
        <span aria-hidden className={label}>{t("skins.model")}</span>
        <div className="flex gap-4">
          {variants.map((v) => (
            <Radio key={v} value={v} className={`group flex h-9 cursor-default items-center gap-2 text-sm text-text ${ring}`}>
              <RadioMark />
              {t(`skins.${v}`)}
            </Radio>
          ))}
        </div>
      </RadioGroup>
      <div className="flex flex-col gap-1.5">
        <span aria-hidden className={label}>{t("skins.file")}</span>
        <Button isDisabled={props.disabled} onPress={() => picker.current?.click()} className={`${secondaryButton} min-w-0 justify-start`}>
          <IconFile aria-hidden className="size-4 shrink-0" />
          <span className="truncate">{props.fileName ?? props.fileLabel}</span>
        </Button>
        <input
          ref={picker}
          type="file"
          accept="image/png"
          aria-label={t("skins.file")}
          hidden
          tabIndex={-1}
          onChange={(e) => {
            const file = e.currentTarget.files?.[0];
            // 同じファイルをもう一度選べるように空にする
            e.currentTarget.value = "";
            if (file) props.onFile(file);
          }}
        />
      </div>
    </>
  );
}
