action(actionId: string | number, data: any) {
    return this._http.patch(`/lol-champ-select/v1/session/actions/${actionId}`, data)
  }

  pickOrBan(championId: number, completed: boolean, type: 'pick' | 'ban', actionId: number) {
    return this.action(actionId, { championId, completed, type })
  }
  private async _pick(championId: number, actionId: number, completed = true) {
    try {
      this._log.info(
        `Now picking: ${this._lc.data.gameData.champions[championId]?.name || championId}, ${this.settings.pickStrategy}, actionId=${actionId}, locked=${completed}`
      )

      await this._lc.api.champSelect.pickOrBan(championId, completed, 'pick', actionId)
    } catch (error) {
      this._ipc.sendEvent(AutoSelectMain.id, 'error-pick', championId)
      this._sendInChat(
        `[League Akari] ${i18next.t('auto-select-main.error-pick', {
          champion: this._lc.data.gameData.champions[championId]?.name || championId,
          reason: formatErrorMessage(error)
        })}`
      )

      this._log.warn(`Failed to pick, target champion: ${championId}`, error)
    }
  }
private _handleAutoPickBan() {
    this._mobx.reaction(
      () =>
        [
          this.state.targetPick,
          this.settings.pickStrategy,
          this.settings.lockInDelaySeconds
        ] as const,
      async ([pick, strategy, delay]) => {
        if (!pick) {
          this._cancelPrevScheduledPickIfExists()
          return
        }

        if (pick.isActingNow && pick.action.isInProgress) {
          if (strategy === 'show') {
            if (this.state.champSelectActionInfo?.memberMe.championId !== pick.championId) {
              this._cancelPrevScheduledPickIfExists()
              await this._pick(pick.championId, pick.action.id, false)
            }
          } else if (strategy === 'lock-in') {
            this._cancelPrevScheduledPickIfExists()
            await this._pick(pick.championId, pick.action.id)
          } else if (strategy === 'show-and-delay-lock-in') {
            if (this.state.champSelectActionInfo?.memberMe.championId !== pick.championId) {
              await this._pick(pick.championId, pick.action.id, false)
            }

            this._cancelPrevScheduledPickIfExists()

            const delayMs = this._calculateAppropriateDelayMs(delay * 1e3)

            this._log.info(
              `Added delayed pick task: ${delay * 1e3} (adjusted: ${delayMs}), target champion: ${this._lc.data.gameData.champions[pick.championId]?.name || pick.championId}`
            )

            this._sendInChat(
              `[${i18next.t('appName')}] ${i18next.t('auto-select-main.delayed-lock-in', {
                champion:
                  this._lc.data.gameData.champions[pick.championId]?.name || pick.championId,
                seconds: (delayMs / 1e3).toFixed(1),
                ns: 'common'
              })}`
            )

            this.state.setUpcomingPick(pick.championId, Date.now() + delayMs)
            this._pickTask.setTask(
              () =>
                this._pick(pick.championId, pick.action.id).finally(() =>
                  this.state.setUpcomingPick(null)
                ),
              true,
              delayMs
            )
          }

          return
        }

        if (!pick.isActingNow) {
          if (!this.settings.showIntent) {
            return
          }

          // 非自定义且未选择英雄
          if (
            this.state.champSelectActionInfo?.session.isCustomGame ||
            this.state.champSelectActionInfo?.memberMe.championId
          ) {
            return
          }

          const thatAction = this.state.champSelectActionInfo?.pick.find(
            (a) => a.id === pick.action.id
          )
          if (thatAction && thatAction.championId === pick.championId) {
            return
          }

          await this._prePick(pick.championId, pick.action.id)
          return
        }
      },
      { equals: comparer.structural }
    )

    this._mobx.reaction(
      () => [this.state.targetBan, this.settings.banDelaySeconds] as const,
      async ([ban, delay]) => {
        if (!ban) {
          this._cancelPrevScheduledBanIfExists()
          return
        }

        if (ban.action.isInProgress && ban.isActingNow) {
          this._cancelPrevScheduledBanIfExists()

          const delayMs = this._calculateAppropriateDelayMs(delay * 1e3)
          this._log.info(
            `Added delayed ban task: ${delay * 1e3} (adjusted: ${delayMs}), target champion: ${this._lc.data.gameData.champions[ban.championId]?.name || ban.championId}`
          )
          this._sendInChat(
            `[${i18next.t('appName')}] ${i18next.t('auto-select-main.delayed-ban', {
              champion: this._lc.data.gameData.champions[ban.championId]?.name || ban.championId,
              seconds: (delayMs / 1e3).toFixed(1),
              ns: 'common'
            })}`
          )
          this.state.setUpcomingBan(ban.championId, Date.now() + delayMs)
          this._banTask.setTask(
            () => {
              this._ban(ban.championId, ban.action.id)
              this.state.setUpcomingBan(null)
            },
            true,
            delayMs
          )
        }
      },
      { equals: comparer.structural }
    )

    // 用于校正时间
    this._mobx.reaction(
      () => this.state.currentPhaseTimerInfo,
      (_timer) => {
        if (this.state.upcomingPick) {
          const adjustedDelayMs = this._calculateAppropriateDelayMs(
            this.settings.lockInDelaySeconds * 1e3
          )

          this._pickTask.updateTime(adjustedDelayMs)
        }

        if (this.state.upcomingBan) {
          const adjustedDelayMs = this._calculateAppropriateDelayMs(
            this.settings.banDelaySeconds * 1e3
          )

          this._banTask.updateTime(adjustedDelayMs)
        }
      }
    )

    this._mobx.reaction(
      () => this.state.upcomingGrab,
      (grab) => {
        this._log.info(`Upcoming Grab - swap scheduled`, grab)
      }
    )

    // for logging only
    const positionInfo = computed(
      () => {
        if (!this.state.champSelectActionInfo) {
          return null
        }

        if (!this.settings.normalModeEnabled || !this.settings.banEnabled) {
          return null
        }

        const position = this.state.champSelectActionInfo.memberMe.assignedPosition

        const championsBan = this.settings.bannedChampions
        const championsPick = this.settings.expectedChampions

        return {
          position,
          ban: championsBan,
          pick: championsPick
        }
      },
      { equals: comparer.structural }
    )

    this._mobx.reaction(
      () => positionInfo.get(),
      (info) => {
        if (info) {
          this._log.info(
            `Assigned position: ${info.position || '<empty>'}, preset pick: ${JSON.stringify(info.pick)}, preset ban: ${JSON.stringify(info.ban)}`
          )
        }
      }
    )

    this._mobx.reaction(
      () => this._lc.data.chat.conversations.championSelect?.id,
      (id) => {
        if (id && this._lc.data.gameflow.phase === 'ChampSelect') {
          if (!this._lc.data.champSelect.session) {
            return
          }

          const texts: string[] = []
          if (!this._lc.data.champSelect.session.benchEnabled && this.settings.normalModeEnabled) {
            texts.push(i18next.t('auto-select-main.auto-pick-normal-mode'))
          }

          if (this._lc.data.champSelect.session.benchEnabled && this.settings.benchModeEnabled) {
            texts.push(i18next.t('auto-select-main.auto-grab-bench-mode'))
          }

          if (!this._lc.data.champSelect.session.benchEnabled && this.settings.banEnabled) {
            let hasBanAction = false
            for (const arr of this._lc.data.champSelect.session.actions) {
              if (arr.findIndex((a) => a.type === 'ban') !== -1) {
                hasBanAction = true
                break
              }
            }
            if (hasBanAction) {
              texts.push(i18next.t('auto-select-main.auto-ban'))
            }
          }

          if (texts.length) {
            this._sendInChat(
              `[League Akari] ${i18next.t('auto-select-main.enabled')} ${texts.join(' | ')}`
            )
          }
        }
      }
    )
  }

private async _ban(championId: number, actionId: number, completed = true) {
    try {
      await this._lc.api.champSelect.pickOrBan(championId, completed, 'ban', actionId)
    } catch (error) {
      this._ipc.sendEvent(AutoSelectMain.id, 'error-ban', championId)
      this._sendInChat(
        `[League Akari] ${i18next.t('auto-select-main.error-ban', {
          champion: this._lc.data.gameData.champions[championId]?.name || championId,
          reason: formatErrorMessage(error)
        })}`
      )

      this._log.warn(`Failed to ban, target champion: ${championId}`, error)
    }
  }
  private async _ban(championId: number, actionId: number, completed = true) {
    try {
      await this._lc.api.champSelect.pickOrBan(championId, completed, 'ban', actionId)
    } catch (error) {
      this._ipc.sendEvent(AutoSelectMain.id, 'error-ban', championId)
      this._sendInChat(
        `[League Akari] ${i18next.t('auto-select-main.error-ban', {
          champion: this._lc.data.gameData.champions[championId]?.name || championId,
          reason: formatErrorMessage(error)
        })}`
      )

      this._log.warn(`Failed to ban, target champion: ${championId}`, error)
    }
  }

  private async _prePick(championId: number, actionId: number) {
    try {
      this._log.info(`Now pre-picking: ${championId}, actionId=${actionId}`)

      await this._lc.api.champSelect.action(actionId, { championId })
    } catch (error) {
      this._ipc.sendEvent(AutoSelectMain.id, 'error-pre-pick', championId)
      this._sendInChat(
        `[League Akari] ${i18next.t('auto-select-main.error-pre-pick', {
          champion: this._lc.data.gameData.champions[championId]?.name || championId,
          reason: formatErrorMessage(error)
        })}`
      )

      this._log.warn(`Failed to pre-pick, target champion: ${championId}`, error)
    }
  }
