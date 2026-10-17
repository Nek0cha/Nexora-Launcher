import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { beginLogin, cancelTask, listAccounts } from "../../api/commands";
import { onLoginProgress } from "../../api/events";
import { toErrorPayload } from "../../api/ipc";
import type { ErrorPayload } from "../../bindings/ErrorPayload";
import type { LoginMethod } from "../../bindings/LoginMethod";
import type { LoginProgress } from "../../bindings/LoginProgress";
import { ACCOUNTS_KEY, pickActive } from "../accounts/useAccounts";
import { isTerminal } from "../tasks/taskTable";
import { useTasks } from "../tasks/useTasks";

export type LoginState = {
  start(method: LoginMethod): void;
  cancel(): void;
  progress: LoginProgress | null;
  taskId: string | null;
  result: { accountId: string } | null;
  error: ErrorPayload | null;
  /** 最後に始めた方式。 */
  method: LoginMethod | null;
  /** 始めてから、成功、失敗、キャンセルのどれかになるまでの間。 */
  running: boolean;
};

const warn = (error: unknown) => console.warn(toErrorPayload(error).code);

/**
 * ログインのタスクを 1 件動かす。進み具合は `login://progress` のうち自分のタスクのものだけを使い、
 * 終わったことはタスクの表で知る。成功したらアカウントの一覧を取り直し、使用中のアカウントを結果にする。
 */
export function useLogin(): LoginState {
  const client = useQueryClient();
  const tasks = useTasks();
  const [taskId, setTaskId] = useState<string | null>(null);
  const [progress, setProgress] = useState<LoginProgress | null>(null);
  const [result, setResult] = useState<{ accountId: string } | null>(null);
  const [error, setError] = useState<ErrorPayload | null>(null);
  const [method, setMethod] = useState<LoginMethod | null>(null);
  const [running, setRunning] = useState(false);

  const taskIdRef = useRef<string | null>(null);
  // 終わりを処理したタスク。同じタスクを二度処理しない
  const handledRef = useRef<string | null>(null);
  // 今のタスクのキャンセルが終わるのを待ってから始める方式（ブラウザからデバイスコードへの切り替え）
  const pendingRef = useRef<LoginMethod | null>(null);
  // start と cancel のたびに進める。古い呼び出しの応答を捨てるため
  const generation = useRef(0);
  // ID が分かる前に届いた進み具合
  const early = useRef(new Map<string, LoginProgress>());

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    onLoginProgress((event) => {
      if (disposed) return;
      if (event.taskId === taskIdRef.current) {
        // 切り替えのためにキャンセルしたタスクの進み具合は、もう見せない
        if (pendingRef.current === null) setProgress(event.progress);
      } else {
        early.current.set(event.taskId, event.progress);
      }
    })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch((e) => console.warn("login progress listen failed", e));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const begin = useCallback((m: LoginMethod) => {
    const gen = ++generation.current;
    taskIdRef.current = null;
    setTaskId(null);
    setProgress(null);
    setResult(null);
    setError(null);
    setMethod(m);
    setRunning(true);
    beginLogin(m).then(
      (id) => {
        if (gen !== generation.current) return;
        taskIdRef.current = id;
        setTaskId(id);
        setProgress(early.current.get(id) ?? null);
        early.current.clear();
      },
      (e) => {
        if (gen !== generation.current) return;
        setError(toErrorPayload(e));
        setRunning(false);
      },
    );
  }, []);

  const succeed = useCallback(() => {
    const gen = generation.current;
    client
      .fetchQuery({ queryKey: ACCOUNTS_KEY, queryFn: listAccounts, staleTime: 0 })
      .then(
        (list) => {
          if (gen !== generation.current) return;
          const active = pickActive(list);
          if (active?.kind === "msa") setResult({ accountId: active.account.id });
          else setError({ code: "auth.no_account", params: {}, detail: null });
        },
        (e) => {
          if (gen !== generation.current) return;
          setError(toErrorPayload(e));
        },
      )
      .finally(() => {
        if (gen === generation.current) setRunning(false);
      });
  }, [client]);

  useEffect(() => {
    if (!taskId || handledRef.current === taskId) return;
    const snapshot = tasks[taskId];
    if (!snapshot || !isTerminal(snapshot)) return;
    handledRef.current = taskId;
    const pending = pendingRef.current;
    pendingRef.current = null;
    if (snapshot.state === "succeeded") {
      succeed();
    } else if (pending) {
      begin(pending);
    } else if (snapshot.state === "failed" && snapshot.error?.code !== "task.cancelled") {
      setError(snapshot.error ?? { code: "unknown", params: {}, detail: null });
      setRunning(false);
    } else {
      setRunning(false);
    }
  }, [tasks, taskId, begin, succeed]);

  const start = useCallback(
    (m: LoginMethod) => {
      const current = taskIdRef.current;
      if (current && handledRef.current !== current) {
        // 終わる前に新しく頼むと、backend は同じタスクの ID を返すため、終わるのを待ってから始める
        if (pendingRef.current === null) cancelTask(current).catch(warn);
        pendingRef.current = m;
        setMethod(m);
        setProgress(null);
        return;
      }
      begin(m);
    },
    [begin],
  );

  const cancel = useCallback(() => {
    generation.current++;
    const current = taskIdRef.current;
    if (current && handledRef.current !== current) cancelTask(current).catch(warn);
    taskIdRef.current = null;
    pendingRef.current = null;
    setTaskId(null);
    setProgress(null);
    setError(null);
    setResult(null);
    setRunning(false);
  }, []);

  return { start, cancel, progress, taskId, result, error, method, running };
}
