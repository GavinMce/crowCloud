{{/*
Base name for the chart.
*/}}
{{- define "crowcloud.name" -}}
{{- .Chart.Name | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Fully qualified app name, prefixed with the release name unless the release
name already contains the chart name.
*/}}
{{- define "crowcloud.fullname" -}}
{{- if contains .Chart.Name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name .Chart.Name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{/*
Common labels applied to every resource.
*/}}
{{- define "crowcloud.labels" -}}
helm.sh/chart: {{ printf "%s-%s" (include "crowcloud.name" .) .Chart.Version | replace "+" "_" }}
{{ include "crowcloud.selectorLabels" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end -}}

{{/*
Selector labels shared by a Deployment and its Pods.
*/}}
{{- define "crowcloud.selectorLabels" -}}
app.kubernetes.io/name: {{ include "crowcloud.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{/*
Component-scoped selector labels (api / operator).
*/}}
{{- define "crowcloud.componentSelectorLabels" -}}
{{ include "crowcloud.selectorLabels" . }}
app.kubernetes.io/component: {{ .component }}
{{- end -}}
