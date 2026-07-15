-- Collapse Project -> ResourceGroup -> Resource into Project -> Resource.
-- No production deployments exist yet, so this drops resource_groups and the
-- resource_group column outright rather than attempting a data migration.

ALTER TABLE resources DROP CONSTRAINT resources_project_resource_group_name_key;
DROP INDEX resources_project_rg_idx;
ALTER TABLE resources DROP COLUMN resource_group;
ALTER TABLE resources ADD CONSTRAINT resources_project_name_key UNIQUE (project, name);
CREATE INDEX resources_project_idx ON resources(project);

DROP TABLE resource_groups;
