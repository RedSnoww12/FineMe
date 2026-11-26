// Adaptateur d'envoi SMTP : TLS obligatoire (implicite sur le port 465, STARTTLS sinon).

import { createTransport } from 'nodemailer';
import type { MailConfig } from './config.ts';
import type { Mailer } from './mail.ts';

const TIMEOUT_MS = 30_000;

export function smtpOptions(config: MailConfig) {
  const implicitTls = config.port === 465;
  return {
    host: config.host,
    port: config.port,
    secure: implicitTls,
    requireTLS: !implicitTls,
    auth: { user: config.user, pass: config.password },
    connectionTimeout: TIMEOUT_MS,
    greetingTimeout: TIMEOUT_MS,
    socketTimeout: TIMEOUT_MS,
  };
}

export function smtpMailer(config: MailConfig): Mailer {
  const transport = createTransport(smtpOptions(config));
  return {
    async send(email) {
      await transport.sendMail({ from: config.from, to: config.to, subject: email.subject, text: email.text });
    },
  };
}
