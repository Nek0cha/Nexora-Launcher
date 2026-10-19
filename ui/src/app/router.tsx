import { createHashRouter } from "react-router";
import { App } from "./App";
import Accounts from "../routes/Accounts";
import Browse from "../routes/Browse";
import Home from "../routes/Home";
import InstanceDetail from "../routes/InstanceDetail";
import Instances from "../routes/Instances";
import Settings from "../routes/Settings";

export const router = createHashRouter([
  {
    path: "/",
    element: <App />,
    children: [
      { index: true, element: <Home /> },
      { path: "instances", element: <Instances /> },
      { path: "instances/:id", element: <InstanceDetail /> },
      { path: "browse", element: <Browse /> },
      { path: "accounts", element: <Accounts /> },
      { path: "settings", element: <Settings /> },
    ],
  },
]);
