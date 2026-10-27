// Choosing another course and group of the timetable already imported (Python
// `ui/group_dialog.py`): nothing is parsed, only the choice is saved.
import type { App } from "../../app";
import { h } from "../../dom";
import { t } from "../../i18n";
import { pairsWord } from "../../lib/calendar";
import { modal } from "../popups";

export function openGroupDialog(app: App): Promise<boolean | null> {
  return modal<boolean>((finish) => {
    let groupId: number | null = app.settings.selected_group_id;
    const courseSelect = h("select", { class: "select", "aria-label": t("import_course"), onchange: () => showGroups(true) });
    const list = h("div", { class: "list", role: "listbox", "aria-label": t("import_group") });
    const choose = h("button", { class: "btn primary", onclick: () => void accept() }, t("group_choose"));

    const accept = async () => {
      if (groupId === null) return;
      await app.backend.selectGroup(groupId);
      finish(true);
    };

    const showGroups = (courseChanged: boolean) => {
      const course = app.courses.find((c) => String(c.course.id) === courseSelect.value);
      const groups = course?.groups ?? [];
      if (courseChanged || !groups.some((g) => g.group.id === groupId)) groupId = groups[0]?.group.id ?? null;
      list.replaceChildren(
        ...groups.map(({ group, lessons }) => {
          const suffix = lessons ? `${lessons} ${pairsWord(lessons)}` : t("import_no_pairs");
          const label = [group.name, group.specialty, suffix].filter(Boolean).join("  ·  ");
          return h("button", {
            role: "option",
            "aria-selected": String(group.id === groupId),
            onclick: (event) => {
              groupId = group.id;
              for (const option of list.children) option.setAttribute("aria-selected", String(option === event.currentTarget));
              choose.disabled = false;
            },
            ondblclick: () => void accept(),
          }, label);
        }),
      );
      choose.disabled = groupId === null;
    };

    courseSelect.replaceChildren(...app.courses.map(({ course }) => h("option", { value: String(course.id), selected: course.id === app.settings.selected_course_id }, course.name)));
    showGroups(false);

    return h(
      "div",
      { class: "dialog group" },
      h("h2", {}, t("group_title")),
      h("p", { class: "hint" }, t("group_hint")),
      h("div", { class: "section" }, t("import_course")),
      courseSelect,
      h("div", { class: "section" }, t("import_group")),
      list,
      h("div", { class: "footer" }, h("button", { class: "btn plain", onclick: () => finish(false) }, t("cancel")), choose),
    );
  }, t("group_title"));
}
